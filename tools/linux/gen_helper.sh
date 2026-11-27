#!/bin/bash
# tools/linux/gen_helper.sh <x86_64|aarch64> (inside a build container, tools/linux/appimage.sh): the game code
# generator (tools/gen.py) frozen with PyInstaller (tools/lasgen.spec) into build/linux/<arch>/helper/lasgen/, shipped
# as lasgen/ in the AppImage: what the setup builds the game code with on the player's machine, which has no Python.
# The same packages as `make gen-helper`, from the container's Python 3.12.
set -euo pipefail
cd "$(dirname "$0")/../.."
ARCH=$1
H=$PWD/build/linux/$ARCH/helper
V=$PWD/build/linux/$ARCH/gen-venv
rm -rf "$H" && mkdir -p "$H"
[ -f "$V/.ok" ] || {
    rm -rf "$V" && python3.12 -m venv "$V"
    "$V/bin/python" -m pip install -q capstone==5.0.9 unicorn==2.1.4 lz4==4.4.5 pyinstaller==6.22.3
    touch "$V/.ok"
}
"$V/bin/python" -m PyInstaller --noconfirm --clean --log-level WARN --distpath "$H" \
    --workpath "$PWD/build/linux/$ARCH/pyinstaller" tools/lasgen.spec
du -sh "$H/lasgen"
