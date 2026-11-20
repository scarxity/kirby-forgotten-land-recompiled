#!/usr/bin/env python3
"""Merge shader variant records (runtime/nvn/shader_prep.h: G / C lines, hashes and pipeline state only) into a list.

    tools/shader_list_merge.py OUT IN [IN ...]

Each IN is a list in the shipped format, a player's cache/shader_seen.txt (its "# shader_seen" header and any other
comment lines are dropped) or a game.log: its "[shader] record G ..." / "... C ..." lines (each record the shipped
list lacks, logged once a run). OUT gets every record once, sorted, so a re-run changes only what's new. OUT is read
too when it exists: records are only ever added.

Only a line with exactly the shape of a record gets in (RECORD_RE, the shape tools/ci/no_game_data.sh checks the list
for); any other line that looks like one is skipped and counted, so a bad line never reaches the list.

    tools/shader_list_merge.py runtime/nvn/shader_variants.txt game.log
"""
import os
import re
import sys

TAG = '[shader] record '
_H = '[0-9a-f]{1,8}'
_G = (f'G vs=[0-9a-f]{{16}} fs=[0-9a-f]{{16}} f={_H} a=({_H}(,{_H})*)? s=({_H}/{_H}(,{_H}/{_H})*)? '
      f'c=({_H}(:{_H})?(,{_H}(:{_H})?)*)? d={_H} n={_H} m={_H} b={_H} B=([0-9a-f]{{16}}(,[0-9a-f]{{16}})*)? '
      f'k={_H} t={_H} p={_H}( x=[0-9a-f]{{1,16}}/[0-9a-f]{{1,16}})?')
_C = 'C cs=[0-9a-f]{16} l=[0-9]{1,7},[0-9]{1,7},[0-9]{1,7}'
RECORD_RE = re.compile(f'{_G}|{_C}')
MAX_LINE = 1000  # (the longest shipped record is 242 characters)


def record_of(line):
    """the record a line holds (a bare record or a "[shader] record ..." log line), else None"""
    line = line.strip()
    k = line.find(TAG)
    if k >= 0:
        line = line[k + len(TAG):]
    return line if line[:1] in ('G', 'C') and line[1:2] == ' ' and len(line) <= MAX_LINE and RECORD_RE.fullmatch(line) else None


def records(path):
    """(the records of a file, how many lines looked like one and were not)"""
    out, bad = [], 0
    with open(path, encoding='utf-8', errors='replace') as f:
        for line in f:
            r = record_of(line)
            if r:
                out.append(r)
            elif TAG in line or (line[:1] in ('G', 'C') and line[1:2] == ' '):
                bad += 1
    return out, bad


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out, ins = sys.argv[1], sys.argv[2:]
    have, skipped = set(), 0
    if os.path.exists(out):
        r, bad = records(out)
        have.update(r)
        skipped += bad
    before = len(have)
    for p in ins:
        r, bad = records(p)
        have.update(r)
        skipped += bad
    with open(out, 'w', encoding='utf-8', newline='\n') as f:
        for r in sorted(have):
            f.write(r + '\n')
    g = sum(1 for r in have if r[0] == 'G')
    print(f'{out}: {len(have)} records ({g} pipelines, {len(have) - g} compute), {len(have) - before} new'
          + (f', {skipped} malformed lines skipped' if skipped else ''))


if __name__ == '__main__':
    main()
