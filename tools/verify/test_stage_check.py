"""Synthetic fixtures for stage_check.py (no game files): python tools/verify/test_stage_check.py"""
import os
import struct
import sys
import tempfile
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import stage_check


def png(path, w, h, rgb):
    rows = b''.join(bytes([2 if y else 0]) + (bytes(rgb) * w if not y else bytes(3 * w)) for y in range(h))  # (Up filter)
    chunk = lambda k, d: struct.pack('>I', len(d)) + k + d + struct.pack('>I', zlib.crc32(k + d))
    with open(path, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) +
                chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def wav(path, value, n=48000 * 2):
    data = struct.pack('<%df' % n, *([value] * n))
    with open(path, 'wb') as f:
        f.write(struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', 36 + len(data), b'WAVE', b'fmt ', 16, 3, 2, 48000, 384000, 8, 32,
                            b'data', len(data)) + data)


LOG = """[vk] t=1.0 frame 100 60.0 fps max 20ms level - (10 draws, 0 skipped, 1 passes)
[fs] read ubin/Step/Map/Cat/Stage/Step01/Attr_Attr.bin.cmp
[vk] t=2.0 frame 200 60.0 fps max 20ms level - (10 draws, 5 skipped, 1 passes)
[ipc] STUB prepo:u cmd 10101 (in 8 bytes, x1 a0 b0 c0) -> ok
[vk] t=3.0 frame 900 60.0 fps max 20ms level - (10 draws, %d skipped, 1 passes)
"""

with tempfile.TemporaryDirectory() as d:
    log, shot, snd = os.path.join(d, 'a.log'), os.path.join(d, 'a.png'), os.path.join(d, 'a.wav')
    open(log, 'w').write(LOG % 0)
    png(shot, 4, 3, (200, 100, 50))
    wav(snd, 0.1)
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['reached'] and r['load_frame'] == 100, r
    assert r['skipped_after_hold'] == 0, 'skips before the hold do not count'
    assert abs(r['luma'] - (200 * 2 + 100 * 5 + 50) / 8) < 0.01, 'the Up-filtered rows repeat the first'
    assert abs(r['audio_rms'] - 0.1) < 1e-6, r['audio_rms']
    assert r['verdict'] == 'PASS', r['verdict']
    assert r['noted'] == {'[ipc] STUB prepo:u cmd 10101': 1}, r['noted']

    open(log, 'w').write(LOG % 3 + 'FATAL: no function at 0 (lr=1 ?+0)\n')
    png(shot, 4, 3, (0, 0, 0))
    wav(snd, 0.0)
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['verdict'] == 'FAIL: fatal, missing function, skipped draws, black, silent', r['verdict']

    r = stage_check.check('Cat/Other/Step01', log, os.path.join(d, 'none.png'), os.path.join(d, 'none.wav'), 600)
    assert r['verdict'] == 'FAIL: step not loaded, fatal, missing function, no capture, no audio capture', r['verdict']
    open(log, 'w').write(LOG % 0 + '*** exception c0000005 accessing 0\n')
    r = stage_check.check('Cat/Stage/Step01', log, os.path.join(d, 'none.png'), snd, 600)
    assert r['fatal'] == '*** exception c0000005 accessing 0' and r['verdict'].startswith('FAIL: fatal'), r

    # draws skipped in frames the engine held off the screen (vk_build_hold) are not seen: only the rest count
    png(shot, 4, 3, (200, 100, 50))
    wav(snd, 0.1)
    open(log, 'w').write((LOG % 7).replace('7 skipped,', '7 skipped (7 in held frames),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 0 and r['verdict'] == 'PASS', r
    open(log, 'w').write((LOG % 7).replace('7 skipped,', '7 skipped (5 in held frames),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 2 and r['verdict'] == 'FAIL: skipped draws', r

    # draws the engine showed late, their shaders still building (vk_build_hold's small holds), are allowed; others are not
    open(log, 'w').write((LOG % 2).replace('2 skipped,', '2 skipped (0 in held frames, 2 late),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 0 and r['late_draws'] == 2 and r['verdict'] == 'PASS', r
    open(log, 'w').write((LOG % 5).replace('5 skipped,', '5 skipped (0 in held frames, 2 late),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 3 and r['verdict'] == 'FAIL: skipped draws', r
    # 0.3.2 never holds a frame: the line only says how many were late
    open(log, 'w').write((LOG % 4).replace('4 skipped,', '4 skipped (4 late),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 0 and r['late_draws'] == 4 and r['verdict'] == 'PASS', r
    open(log, 'w').write((LOG % 4).replace('4 skipped,', '4 skipped (1 late),'))
    r = stage_check.check('Cat/Stage/Step01', log, shot, snd, 600)
    assert r['skipped_after_hold'] == 3 and r['verdict'] == 'FAIL: skipped draws', r
print('stage_check: ok')
