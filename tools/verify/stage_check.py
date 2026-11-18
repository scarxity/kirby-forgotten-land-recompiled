"""stage_check.py: judge one run of the stage tour (tools/verify/stage_tour.sh), and sum the runs up. Standard library only.

  stage_check.py check <stage> <log> <shot.png> <audio.wav> <hold frames>   -> one JSON line (the stage's record)
  stage_check.py report <tour.jsonl>                                        -> a Markdown table; exit 1 if any stage failed

A stage passes when its step's files were read (LARECOMP_STAGE took it there), nothing fatal (FATAL or a crash), no missing function, no shader
that failed to translate, no hang dump, no skipped draw in a frame shown once <hold frames> have passed since the step loaded (the
shaders had time to build) beyond those the engine reports it showed late (a few draws whose shaders still build), the final window image
is not black, and the game made sound in the last seconds. Unimplemented services and stubbed commands are listed, not failed: some are
expected offline.
"""
import json
import re
import struct
import sys
import zlib

VK = re.compile(r'\[vk\] t=[\d.]+ frame (\d+) .*?\((\d+) draws, (\d+) skipped(?: \((?:(\d+) in held frames)?(?:, )?(?:(\d+) late)?\))?')
STEP_READ = '[fs] read ubin/Step/Map/'
NOTED = ('[hle] UNIMPLEMENTED', '[ipc] STUB', '(unhandled')


def png_luma(path):
    """Mean luma (0-255) of an 8-bit RGB/RGBA PNG, None if unreadable."""
    try:
        data = open(path, 'rb').read()
    except OSError:
        return None
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        return None
    pos, idat, w = 8, b'', 0
    while pos < len(data):
        n, kind = struct.unpack('>I4s', data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b'IHDR':
            w, h, depth, ctype, _, _, interlace = struct.unpack('>IIBBBBB', body)
            if depth != 8 or ctype not in (2, 6) or interlace:
                return None
            bpp = 3 if ctype == 2 else 4
        elif kind == b'IDAT':
            idat += body
        pos += 12 + n
    raw, stride = zlib.decompress(idat), w * bpp
    prev, total, row_at = bytearray(stride), 0, 0
    for _ in range(h):
        f, line = raw[row_at], bytearray(raw[row_at + 1:row_at + 1 + stride])
        row_at += 1 + stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b, c = prev[i], prev[i - bpp] if i >= bpp else 0
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        total += sum(line[0::bpp]) * 2 + sum(line[1::bpp]) * 5 + sum(line[2::bpp])
        prev = line
    return total / (8 * w * h)


def wav_tail_rms(path, seconds=5.0):
    """RMS of the last seconds of the runtime's float WAV (LARECOMP_AUDIO_WAV), None if absent."""
    try:
        data = open(path, 'rb').read()
    except OSError:
        return None
    if len(data) < 44 or data[:4] != b'RIFF':
        return None
    ch, rate = struct.unpack_from('<HI', data, 22)
    samples = data[44:]
    take = min(len(samples) // 4, int(seconds * rate * ch))
    if take == 0:
        return 0.0
    vals = struct.unpack_from('<%df' % take, samples, len(samples) - take * 4 - len(samples) % 4)
    return (sum(v * v for v in vals) / take) ** 0.5


def check(stage, log_path, shot, wav, hold):
    rec = {'stage': stage, 'reached': False, 'load_frame': None, 'last_frame': 0, 'fatal': None, 'missing_fn': 0,
           'translate_failed': 0, 'hang': False, 'timed_out': False, 'skipped_after_hold': 0, 'late_draws': 0, 'held_frames': 0, 'noted': {}}
    stats = []
    with open(log_path, 'r', encoding='utf-8', errors='replace') as f:
        for line in f:
            if line.startswith(STEP_READ + stage + '/'):
                rec['reached'] = True
                if rec['load_frame'] is None:
                    rec['load_frame'] = stats[-1][0] if stats else 0
            m = VK.search(line)
            if m:
                stats.append((int(m.group(1)), max(0, int(m.group(3)) - int(m.group(4) or 0)), int(m.group(5) or 0)))
                rec['last_frame'] = int(m.group(1))
            if (line.startswith('FATAL') or line.startswith('*** exception')) and not rec['fatal']:
                rec['fatal'] = line.strip()[:200]
            if 'no function at' in line:
                rec['missing_fn'] += 1
            if 'translate failed' in line:
                rec['translate_failed'] += 1
            if line.startswith('[hang]'):
                rec['hang'] = True
            if line.startswith('[tour] killed'):
                rec['timed_out'] = True
            for key in NOTED:
                if key in line:
                    k = re.sub(r'\(.*|->.*|x0=.*', '', line.strip()).strip()[:90]
                    rec['noted'][k] = rec['noted'].get(k, 0) + 1
    if rec['load_frame'] is not None:
        start = rec['load_frame'] + hold
        rec['late_draws'] = sum(n for fr, s, n in stats if fr >= start)
        rec['skipped_after_hold'] = sum(max(0, s - n) for fr, s, n in stats if fr >= start)
        rec['held_frames'] = max(0, rec['last_frame'] - rec['load_frame'])
    rec['luma'] = png_luma(shot)
    rec['audio_rms'] = wav_tail_rms(wav)
    why = []
    if not rec['reached']: why.append('step not loaded')
    if rec['fatal']: why.append('fatal')
    if rec['missing_fn']: why.append('missing function')
    if rec['translate_failed']: why.append('translate failed')
    if rec['hang']: why.append('hang')
    if rec['timed_out']: why.append('timed out')
    if rec['reached'] and rec['held_frames'] < hold: why.append('held %d of %d frames' % (rec['held_frames'], hold))
    if rec['skipped_after_hold']: why.append('skipped draws')
    if rec['luma'] is None: why.append('no capture')
    elif rec['luma'] < 8: why.append('black')
    if rec['audio_rms'] is None: why.append('no audio capture')
    elif rec['audio_rms'] < 1e-4: why.append('silent')
    rec['verdict'] = 'PASS' if not why else 'FAIL: ' + ', '.join(why)
    return rec


def report(path):
    recs = [json.loads(l) for l in open(path, encoding='utf-8') if l.strip()]
    out = ['| stage | verdict | load frame | held | skipped | luma | audio rms | notes |', '|---|---|---|---|---|---|---|---|']
    for r in recs:
        notes = '; '.join('%s x%d' % kv for kv in sorted(r['noted'].items()))[:160]
        luma = '-' if r['luma'] is None else '%.0f' % r['luma']
        rms = '-' if r['audio_rms'] is None else '%.4f' % r['audio_rms']
        out.append('| %s | %s | %s | %d | %d | %s | %s | %s |' % (r['stage'], r['verdict'], r['load_frame'], r['held_frames'],
                                                             r['skipped_after_hold'], luma, rms, notes))
    passed = sum(r['verdict'] == 'PASS' for r in recs)
    out.append('')
    out.append('%d of %d stages passed' % (passed, len(recs)))
    print('\n'.join(out))
    return 0 if recs and passed == len(recs) else 1


if __name__ == '__main__':
    if len(sys.argv) == 7 and sys.argv[1] == 'check':
        print(json.dumps(check(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], int(sys.argv[6]))))
    elif len(sys.argv) == 3 and sys.argv[1] == 'report':
        sys.exit(report(sys.argv[2]))
    else:
        sys.exit(__doc__)
