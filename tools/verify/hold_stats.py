"""hold_stats.py <tour out folder>...: the stutter of stage tour runs (tools/verify/stage_tour.sh), from their logs.

Frames the engine held for shader builds (vk_build_hold), split into area-entry bursts (100 or more draws waiting: a new
area's shaders) and the rest (what a player feels as stutter), and the frames shown with a few draws late. Compare a change on a cold cache: LARECOMP_VK_COLD=1 and an empty <work>/cache, the same stages, several runs a
side, alternating. The engine logs the first 200 held frames of a run: keep runs short (the stage tour's are).
Standard library only.
"""
import glob
import os
import re
import sys

HELD = re.compile(r'\[vk\] frame held (\d+) ms for \d+ shader / pipeline builds \((\d+) draws waiting')
SHOWN = re.compile(r'\[vk\] frame shown after \d+ ms with (\d+) draws missing')
LATE = re.compile(r'\[vk\] t=.* skipped \((?:\d+ in held frames, )?(\d+) late\)')


def stats(folder):
    s = {'bursts': 0, 'burst_ms': 0, 'holds': 0, 'hold_ms': 0, 'long': 0, 'late_frames': 0, 'late_draws': 0}
    for path in sorted(glob.glob(os.path.join(folder, '*.log'))):
        with open(path, encoding='utf-8', errors='replace') as f:
            for line in f:
                m = HELD.match(line)
                if m:
                    ms, waiting = int(m.group(1)), int(m.group(2))
                    if waiting >= 100:
                        s['bursts'] += 1
                        s['burst_ms'] += ms
                    else:
                        s['holds'] += 1
                        s['hold_ms'] += ms
                        s['long'] += ms >= 50
                if SHOWN.match(line):
                    s['late_frames'] += 1
                m = LATE.match(line)
                if m:
                    s['late_draws'] += int(m.group(1))
    return s


if __name__ == '__main__':
    for folder in sys.argv[1:]:
        s = stats(folder)
        print(f"{folder}: holds {s['holds']} ({s['hold_ms']} ms, {s['long']} of 50 ms or more); "
              f"area-entry bursts {s['bursts']} ({s['burst_ms']} ms); "
              f"{s['late_frames']} frames shown with {s['late_draws']} draws late")
