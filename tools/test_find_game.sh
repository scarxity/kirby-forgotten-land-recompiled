#!/bin/bash
# test_find_game.sh <runtime exe>: runs `--check` on synthetic game dumps (a few bytes, no game data) built in a temp
# folder and compares its verdict with what the setup must decide. Run it with `make test-find-game` in Git Bash.
set -u
EXE=${1:?usage: test_find_game.sh <runtime exe>}
command -v cygpath >/dev/null || cygpath() { echo "$2"; }  # (Linux: paths are as they are)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
export LARECOMP_DATA_ROOT=$(cygpath -w "$T/data")
mkdir "$T/data"

# main.npdm: "META", the offset of ACI0 at 0x70, ACI0 with the program id at +0x10 (the game's, or $1 when given)
npdm() {
    { printf 'META'; head -c 108 /dev/zero; printf '\x80\0\0\0'; head -c 12 /dev/zero
      printf 'ACI0'; head -c 12 /dev/zero; printf "${1:-\x00\xe0\x5a\x0c\x30\x4d\x00\x01}"; } > "$2/main.npdm"
}
nso() {  # an exefs/main with the main build ID of the version in runtime/game_addresses/versions.h, and no code
    local id; id=$(sed -n 's/^GA_VERSION([^"]*"[^"]*", "\([0-9A-F]*\)").*/\1/p' runtime/game_addresses/versions.h | head -1)
    { printf 'NSO0'; head -c 60 /dev/zero; printf "$(echo "$id" | sed 's/../\\x&/g')"; head -c 12 /dev/zero; } > "$1"
}
exefs() { mkdir -p "$1"; nso "$1/main"; npdm "${2:-}" "$1"; }
romfs() { mkdir -p "$1/msg" "$1/gfx"; }

fail=0
expect() {  # expect <verdict> <folder>
    local got
    got=$("$EXE" --check "$(cygpath -w "$2")" 2>/dev/null | tr -d '\r')
    if [ "$got" = "$1" ]; then echo "pass: $(basename "$2") -> $got"; else echo "FAIL: $(basename "$2"): wanted '$1', got '$got'"; fail=1; fi
}

d=$T/plain;      exefs $d/exefs;           romfs $d/romfs;           expect ok "$d"
d=$T/named;      exefs "$d/Kirby exefs";   romfs "$d/Kirby romfs";   expect ok "$d"
d=$T/nested;     exefs $d/dump/exefs;      romfs $d/dump/romfs;      expect ok "$d"
d=$T/flat;       exefs $d;                 romfs $d;                 expect ok "$d"
d=$T/image;      exefs $d/exefs;           echo x > $d/romfs.bin;    expect ok "$d"
d=$T/many;       for i in $(seq 100); do mkdir -p $d/aaa$i; done; exefs $d/zzz/exefs; romfs $d/zzz/romfs; expect ok "$d"
d=$T/version;    exefs $d/exefs '\x01\x02\x03\x04\x05\x06\x07\x08'; romfs $d/romfs; expect "failed version" "$d"
d=$T/otherbuild; exefs $d/exefs; echo code > $d/exefs/main; romfs $d/romfs; expect "failed version" "$d"
d=$T/update;     exefs $d/exefs '\x00\xe8\x5a\x0c\x30\x4d\x00\x01'; romfs $d/romfs; expect "failed update" "$d"
d=$T/package;    mkdir $d; : > $d/Kirby.nsp;                         expect "failed package" "$d"
d=$T/empty;      mkdir $d;                                           expect "failed not_found" "$d"
d=$T/noromfs;    exefs $d/exefs;                                     expect "failed not_found" "$d"
d=$T/halfromfs;  exefs $d/exefs; mkdir -p $d/romfs/msg;              expect "failed not_found" "$d"
d=$T/missing;                                                        expect "failed not_found" "$d"
exit $fail
