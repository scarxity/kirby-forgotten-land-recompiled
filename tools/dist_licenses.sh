#!/bin/bash
# usage: tools/dist_licenses.sh <staged folder> (make dist, which passes PY): the licence of everything shipped, into
# <folder>/licenses/. Every DLL next to the exes must have a licence below: an unknown one stops the build.
set -euo pipefail
cd "$(dirname "$0")/.."
S=$1 L=$1/licenses D=build M=/ucrt64/share/licenses
PY=${PY:?run it through make dist}
SITE=$(cygpath -u "$("$PY" -c 'import sysconfig; print(sysconfig.get_paths()["purelib"])')")
PYBASE=$(cygpath -u "$("$PY" -c 'import sys; print(sys.base_prefix)')")
FF=$D/deps-win/src/ffmpeg
mkdir -p "$L"

# the project and what is compiled into ForgottenLandRecomp.exe
cp LICENSE-LASRecomp "$L/LASRecomp-MIT.txt"
cp third_party/imgui/LICENSE.txt "$L/DearImGui-MIT.txt"
cp third_party/smaa/LICENSE.txt "$L/SMAA-MIT.txt"
cp third_party/ryujinx/LICENSE.txt "$L/Ryujinx-MIT.txt"  # (code adapted in the shader translator and the audio renderer)
# fonts and runtime DLLs
cp third_party/fonts/Baloo2/OFL.txt "$L/Baloo2-OFL-1.1.txt"
cp third_party/fonts/DejaVu/LICENSE "$L/DejaVu-LICENSE.txt"
cp "$FF/COPYING.LGPLv2.1" "$L/FFmpeg-LGPL-2.1.txt"
cp $M/SDL3/LICENSE.txt "$L/SDL3-zlib.txt"
cp $M/shaderc/LICENSE "$L/shaderc-Apache-2.0.txt"
cp $M/glslang/LICENSE.txt "$L/glslang.txt"  # (built into libshaderc_shared.dll)
cp $M/spirv-tools/LICENSE "$L/SPIRV-Tools-Apache-2.0.txt"
cp $M/zstd/LICENSE "$L/zstd-BSD.txt"  # (linked into the runtime: the pre-build reads the compressed game files)
cp $M/libiconv/COPYING.LIB "$L/libiconv-LGPL.txt"
cp $M/libgcc/COPYING.RUNTIME "$L/GCC-runtime-library-exception.txt"
cp $M/libgcc/COPYING3 "$L/GCC-GPL-3.0.txt"
cp $M/libwinpthread/COPYING "$L/winpthreads.txt"
# zig/: zig (MIT) with LLVM, clang and lld inside it, clang's headers, MinGW-w64's headers and runtime objects
cp "$D/zig/LICENSE" "$L/Zig-MIT.txt"
cp $M/llvm-libs/LICENSE "$L/LLVM-Apache-2.0-with-LLVM-exception.txt"
cp $M/crt/COPYING.MinGW-w64-runtime.txt "$L/MinGW-w64-runtime.txt"
cp $M/crt/COPYING.MinGW-w64.txt "$L/MinGW-w64.txt"
# lasgen/: Python and the frozen generator's libraries
cp "$PYBASE/LICENSE.txt" "$L/Python-PSF.txt"  # (with the notices of what Python bundles: libffi, bzip2, xz, ...)
cp "$SITE"/pyinstaller-*.dist-info/licenses/COPYING.txt "$L/PyInstaller-bootloader-GPL-2.0-with-exception.txt"
cp "$SITE"/capstone-*.dist-info/LICENSE.TXT "$L/capstone-BSD-3-Clause.txt"
cp "$SITE"/lz4-*.dist-info/licenses/LICENSE "$L/python-lz4-BSD-3-Clause.txt"
cp "$FF/COPYING.GPLv2" "$L/unicorn-GPL-2.0.txt"  # (unicorn.dll is GPL-2.0, its Python binding BSD; its wheel has no licence file)
# ForgottenLandLauncher.exe: every crate and npm package in it
"$PY" -I tools/launcher_notice.py launcher "$L/ForgottenLandLauncher-third-party.txt"

for dll in "$S"/*.dll; do
    case $(basename "$dll") in
        avcodec-*|avformat-*|avutil-*|swresample-*|swscale-*|SDL3.dll|libshaderc_shared.dll|libiconv-2.dll) ;;
        libgcc_s_seh-1.dll|libstdc++-6.dll|libwinpthread-1.dll) ;;
        *) echo "dist_licenses.sh: add the licence of $(basename "$dll")"; exit 1 ;;
    esac
done
echo "$L: $(ls "$L" | wc -l) files"
