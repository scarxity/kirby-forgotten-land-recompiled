#!/bin/sh
# Fails if the git tree holds anything of the game: files with a Switch game extension, romfs / exefs / generated
# folders, the game library, keys, or a file over 1 MiB outside third_party/. The repo never holds game data and CI
# never sees any, so a hit is a mistake to fix before it is pushed. Checks what git tracks (what a push publishes).
cd "$(dirname "$0")/../.." || exit 2
fail=0
hits=$(git ls-files | grep -E -i \
  '(^|/)(romfs|exefs|generated[^/]*|extracted[^/]*)/|(^|/)(lasgame\.[^/]*|romfs\.bin|[^/]*\.keys)$|\.(nso|nca|nsp|xci|bfsha|bfres|zs|cmp)$')
if [ -n "$hits" ]; then
  echo "game data in the tree:"; echo "$hits" | sed 's/^/  /'; fail=1
fi
# The shipped shader list (runtime/nvn/shader_variants.txt) holds variant records only: a shader's identity as a
# 16-digit hash, the rest pipeline state in hex or decimal (runtime/nvn/shader_prep.c shader_rec_format). Any other line
# (a path, a comment, shader code) fails.
H='[0-9a-f]{1,8}'
G="^G vs=[0-9a-f]{16} fs=[0-9a-f]{16} f=$H a=($H(,$H)*)? s=($H/$H(,$H/$H)*)? c=($H(:$H)?(,$H(:$H)?)*)? d=$H n=$H m=$H b=$H"
G="$G B=([0-9a-f]{16}(,[0-9a-f]{16})*)? k=$H t=$H p=$H( x=[0-9a-f]{1,16}/[0-9a-f]{1,16})?\$"
bad=$(grep -n -v -E -e "$G" -e '^C cs=[0-9a-f]{16} l=[0-9]{1,7},[0-9]{1,7},[0-9]{1,7}$' runtime/nvn/shader_variants.txt | cut -c1-120)
if [ -n "$bad" ]; then
  echo "runtime/nvn/shader_variants.txt: lines that aren't shader variant records:"; echo "$bad" | sed 's/^/  /'; fail=1
fi
big=$(git ls-files -z | grep -z -v '^third_party/' | xargs -0 wc -c | grep -v ' total$' | awk '$1 > 1048576')
if [ -n "$big" ]; then
  echo "files over 1 MiB outside third_party/:"; echo "$big" | sed 's/^/  /'; fail=1
fi
[ "$fail" = 0 ] && echo "no game data in the tree ($(git ls-files | wc -l) files checked)"
exit $fail
