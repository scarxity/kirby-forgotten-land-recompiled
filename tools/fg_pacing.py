"""Frame pacing from LARECOMP_FRAMETIME_CSV (runtime/nvn/vulkan/frametime.c), with or without Frame Generation.

Usage: python tools/fg_pacing.py <frametimes.csv> [--log <game log>] [--from-frame N] [--to-frame N]

Each image is held on screen from its present until the next one: the hold. Even pacing is every hold the same length
(60 images a second: 16.7 ms each). Holds are taken from the display times (shown_ms, VK_KHR_present_wait) when the CSV
has them, else from the present calls, which are only the app's side of it. Printed:
  images/s by kind; hold P1/P5/P50/P95/P99/max; spread (P95 / P5 of the hold: 1.0 is even); holds over 50 ms;
  game frames without an in-between, and per frame the in-between's share of the frame's time on screen (0.5: half way);
  with --log, the [interp] counters over the same run: cuts and frames without in-betweens by reason, draws matched,
  replay geometry binds not found.
"""
import argparse
import csv
import math
import re
from collections import Counter


def pct(sorted_values, p):
    return sorted_values[max(0, math.ceil(len(sorted_values) * p / 100) - 1)]


def log_counters(path, first, last):
    cuts, notmade = Counter(), Counter()
    draws = matched = binds = notfound = windows = 0
    frame = 0
    for line in open(path, encoding="utf-8", errors="replace"):
        m = re.match(r"\[vk\] t=[\d.]+ frame (\d+)", line)
        if m:
            frame = int(m.group(1))
        if not first <= frame <= last:
            continue
        m = re.search(r"\[interp\] \S+: [\d.]+ game frames/s, draws (\d+)/frame matched ([\d.]+)%", line)
        if m:
            windows += 1
            draws += int(m.group(1))
            matched += int(m.group(1)) * float(m.group(2)) / 100
            if "cuts:" in line:
                for k, v in re.findall(r"([a-z][a-z /]*?) (\d+)", line.split("cuts:")[1]):
                    cuts[k.strip()] += int(v)
        m = re.search(r"\[interp\] presents .* frames without in-betweens:(.*)", line)
        if m:
            for k, v in re.findall(r"([a-zA-Z][a-zA-Z ]*?) (\d+)", m.group(1)):
                notmade[k.strip()] += int(v)
        m = re.search(r"replay geometry: (\d+) binds .*, (\d+) not found", line)
        if m:
            binds += int(m.group(1))
            notfound += int(m.group(2))
    print(f"log (same frames): {windows} stats windows; draws matched {100 * matched / max(draws, 1):.1f}%; "
          f"replay geometry binds not found {notfound}/{binds} ({100 * notfound / max(binds, 1):.1f}%)")
    print(f"  cuts: {dict(cuts)}")
    print(f"  frames without in-betweens: {dict(notmade)}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("csv_path")
    ap.add_argument("--log")
    ap.add_argument("--from-frame", type=int, default=0)
    ap.add_argument("--to-frame", type=int, default=1 << 62)
    ap.add_argument("--from-event", help="with --log: start at the frame the input script anchored this event "
                    "(\"[input] anchor @<event> at frame N\"), plus --from-frame")
    ap.add_argument("--hz", type=float, help="the display's refresh rate: how many holds are whole refreshes (+-1 ms)")
    a = ap.parse_args()
    if a.from_event:
        anchors = [int(m.group(1)) for line in open(a.log, encoding="utf-8", errors="replace")
                   for m in [re.search(r"\[input\] anchor @" + re.escape(a.from_event) + r" at frame (\d+)", line)] if m]
        if not anchors:
            ap.error(f"no anchor @{a.from_event} in the log")
        a.from_frame += anchors[0]
        print(f"from frame {a.from_frame} (@{a.from_event})")
    rows = [r for r in csv.DictReader(open(a.csv_path, newline="")) if a.from_frame <= int(r["present"]) <= a.to_frame]
    if len(rows) < 3:
        ap.error("fewer than 3 presents in the range")
    shown = all(float(r["shown_ms"]) >= 0 for r in rows[: len(rows) // 2]) and \
        sum(float(r["shown_ms"]) >= 0 for r in rows) > 0.95 * len(rows)
    col = "shown_ms" if shown else "t_ms"
    rows = [r for r in rows if float(r[col]) >= 0]
    times = [float(r[col]) for r in rows]
    holds = [b - a_ for a_, b in zip(times, times[1:])]
    span = (times[-1] - times[0]) / 1000
    kinds = Counter(r["kind"] for r in rows)
    print(f"{len(rows)} presents over {span:.1f} s, holds from {'the display (present wait)' if shown else 'present calls (no display times)'}")
    print("  images/s: " + ", ".join(f"{k} {n / span:.1f}" for k, n in sorted(kinds.items())))
    h = sorted(holds)
    print(f"  hold ms: P1 {pct(h, 1):.2f} P5 {pct(h, 5):.2f} P50 {pct(h, 50):.2f} P95 {pct(h, 95):.2f} "
          f"P99 {pct(h, 99):.2f} max {h[-1]:.2f}; spread P95/P5 {pct(h, 95) / max(pct(h, 5), 0.01):.2f}; "
          f"over 50 ms {sum(x > 50 for x in holds)}")
    if a.hz:
        period = 1000 / a.hz
        on_grid = sum(abs(x - round(x / period) * period) <= 1 for x in holds)
        refreshes = Counter(max(1, round(x / period)) for x in holds)
        print(f"  on the {a.hz:g} Hz refresh grid: {100 * on_grid / len(holds):.0f}% of holds; holds in refreshes: "
              + ", ".join(f"{k}: {100 * n / len(holds):.0f}%" for k, n in sorted(refreshes.items())[:8]))
    # per game frame: its in-betweens' share of its time on screen (the frame's images: its mids, then its game image)
    frames, share, without = {}, [], 0
    for i, r in enumerate(rows[:-1]):
        if r["kind"] == "repeat":
            continue
        frames.setdefault(int(r["present"]), []).append((r["kind"], holds[i]))
    for imgs in frames.values():
        mid = sum(d for k, d in imgs if k == "mid")
        total = sum(d for _, d in imgs)
        if not any(k == "mid" for k, _ in imgs):
            without += 1
        elif total > 0:
            share.append(mid / total)
    s = sorted(share)
    print(f"  game frames {len(frames)}, without an in-between {without} ({100 * without / max(len(frames), 1):.1f}%)"
          + (f"; in-between share of the frame's screen time P5 {pct(s, 5):.2f} P50 {pct(s, 50):.2f} P95 {pct(s, 95):.2f}" if s else ""))
    if a.log:
        log_counters(a.log, a.from_frame, a.to_frame)


if __name__ == "__main__":
    main()
