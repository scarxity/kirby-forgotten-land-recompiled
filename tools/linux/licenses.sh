#!/bin/bash
# tools/linux/licenses.sh <x86_64|aarch64> <folder> (tools/linux/package.sh): the licence of everything the AppImage ships,
# into <folder>. What deps/ built keeps its licences in deps/licenses (tools/linux/build_deps.sh). A missing one stops the build.
set -euo pipefail
cd "$(dirname "$0")/../.."
ARCH=$1 L=$2
P=$PWD/build/linux/$ARCH/deps
V=$PWD/build/linux/$ARCH/gen-venv
mkdir -p "$L"
get() {  # name source: copied when it is there, else the build stops
    [ -f "$2" ] || { echo "licenses.sh: no $2 for $1"; exit 1; }
    cp "$2" "$L/$1"
}
# the project and what is compiled into the runtime
get LASRecomp-MIT.txt LICENSE-LASRecomp
get DearImGui-MIT.txt third_party/imgui/LICENSE.txt
get SMAA-MIT.txt third_party/smaa/LICENSE.txt
get Ryujinx-MIT.txt third_party/ryujinx/LICENSE.txt
get Baloo2-OFL-1.1.txt third_party/fonts/Baloo2/OFL.txt
get DejaVu-LICENSE.txt third_party/fonts/DejaVu/LICENSE
get zstd-BSD.txt /usr/share/doc/libzstd1/copyright  # (linked statically: the pre-build reads the compressed game files)
# the libraries built from source in deps/
for f in FFmpeg-LGPL-2.1.txt SDL3-zlib.txt shaderc-Apache-2.0.txt glslang.txt SPIRV-Tools-Apache-2.0.txt; do get "$f" "$P/licenses/$f"; done
# zig/: zig (MIT) with LLVM, clang and lld inside it
get Zig-MIT.txt build/zig/LICENSE
get LLVM-Apache-2.0-with-LLVM-exception.txt "$(ls /usr/share/doc/llvm-21*/copyright /usr/lib/llvm-21/share/doc/*/LICENSE.TXT 2>/dev/null | head -n 1)"
# lasgen/: Python and the frozen generator's libraries
PYBASE=$(python3.12 -c 'import sys; print(sys.base_prefix)')
SITE=$("$V/bin/python" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')
get Python-PSF.txt "$(ls "$PYBASE"/lib/python3.12/LICENSE.txt 2>/dev/null | head -n 1)"
get PyInstaller-bootloader-GPL-2.0-with-exception.txt "$(ls "$SITE"/pyinstaller-*.dist-info/licenses/COPYING.txt | head -n 1)"
get capstone-BSD-3-Clause.txt "$(ls "$SITE"/capstone-*.dist-info/LICENSE.TXT | head -n 1)"
get python-lz4-BSD-3-Clause.txt "$(ls "$SITE"/lz4-*.dist-info/licenses/LICENSE | head -n 1)"
get unicorn-GPL-2.0.txt "$P/licenses/FFmpeg-COPYING.GPLv2"  # (unicorn's library is GPL-2.0, its Python binding BSD; its wheel has no licence file)
echo "$L: $(ls "$L" | wc -l) files"
