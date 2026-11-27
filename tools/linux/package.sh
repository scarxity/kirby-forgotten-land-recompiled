#!/bin/bash
# tools/linux/package.sh <x86_64|aarch64> (inside a build container, tools/linux/appimage.sh: the AppImage's own arch
# when the host has it, else amd64 cross-compiling): the runtime built against build/linux/<arch>/deps and the game
# code generator (tools/linux/gen_helper.sh) put together as an AppDir and packed by appimagetool:
# build/dist/ForgottenLandRecomp-<version>-Linux-<arch>.AppImage
# Shipped next to the executable (usr/bin, where runtime/paths.c rt_resource looks): lasgen/ and zig/, what the setup
# builds the game code with on the player's machine (runtime/gamebuild.c), gamelib-src/, the window icon, the fonts,
# README.txt and licenses/. No game code and no game files: the game code is built from the player's own files.
# usr/lib: the libraries from deps/ only - glibc, libstdc++, the Vulkan loader and the GPU drivers are the system's (a
# bundled loader would not find the user's drivers).
set -euo pipefail
cd "$(dirname "$0")/../.."
ARCH=$1
L=$PWD/build/linux/$ARCH
P=$L/deps
B=$L/game
APPDIR=$L/AppDir
VERSION=$(sed -n 's/^GAME_VERSION := //p' game/game.mk)
EXE_NAME=$(sed -n 's/^EXE_NAME *:= *//p' game/game.mk)
OUT="build/dist/$EXE_NAME-$VERSION-Linux-$ARCH.AppImage"
JOBS=$(nproc)

tools/linux/build_deps.sh "$ARCH"

if [ "$ARCH" != "$(uname -m)" ]; then
    : "${SYSROOT:?cross build: SYSROOT (the arm64 root filesystem)}"
    MAKEARGS=(ARCH="$ARCH" SYSROOT="$SYSROOT")
else
    MAKEARGS=(ARCH="$ARCH")
fi

# the app alone (the game code is built on the player's machine)
make -j"$JOBS" B="$B" DEPS="$P" SHADERC=-lshaderc_shared "${MAKEARGS[@]}" "$B/$EXE_NAME"
if nm "$B/$EXE_NAME" | grep ' f_70' >/dev/null; then echo "$B/$EXE_NAME has game code linked in"; exit 1; fi
ZIG_TARGET=$ARCH-linux tools/fetch_zig.sh

# ---- AppDir
rm -rf "$APPDIR" && mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib"
cp "$B/$EXE_NAME" "$APPDIR/usr/bin/"
llvm-strip --strip-debug "$APPDIR/usr/bin/$EXE_NAME"  # (symbols stay: crash backtraces name functions)
cp docs/icon/icon_1024.png "$APPDIR/usr/bin/icon.png"
cp third_party/fonts/Baloo2/Baloo2-SemiBold.ttf "$APPDIR/usr/bin/"  # (in-game menu font, OFL)
cp third_party/fonts/DejaVu/DejaVuSans.ttf third_party/fonts/DejaVu/DejaVuSans-Bold.ttf "$APPDIR/usr/bin/"  # (last-resort fonts, free license)
cp LICENSE "$APPDIR/usr/bin/LICENSE.txt"
cp recomplauncher.json "$APPDIR/usr/bin/"
[ -d "$L/helper/lasgen" ] || { echo "no game code generator helper: tools/linux/gen_helper.sh $ARCH first"; exit 1; }
cp -R "$L/helper/lasgen" build/zig "$APPDIR/usr/bin/"
mkdir "$APPDIR/usr/bin/gamelib-src"
cp runtime/cpu.h runtime/simd.h runtime/simd_fp.h runtime/gamelib.h tools/gamelib_entry.c runtime/hle/hle_symbols.txt \
   "$APPDIR/usr/bin/gamelib-src/"
cp tools/linux/AppRun "$APPDIR/" && chmod +x "$APPDIR/AppRun"
sed "s/__EXE__/$EXE_NAME/" tools/linux/ForgottenLand.desktop > "$APPDIR/ForgottenLand.desktop"
cp docs/icon/icon_1024.png "$APPDIR/ForgottenLand.png" && ln -sf ForgottenLand.png "$APPDIR/.DirIcon"
# (the FFmpeg version and configure line it was built with: what the LGPL asks to make it rebuildable)
sed -e "s|__FFVER__|$(cat "$P/ffmpeg-version.txt")|g" -e "/__FFCONF__/{r $P/ffmpeg-configure.txt" -e "d}" -e "s|__VERSION__|$VERSION|g" \
    docs/release-README-linux.txt > "$APPDIR/usr/bin/README.txt"
! grep '__[A-Z]*__' "$APPDIR/usr/bin/README.txt"
tools/linux/licenses.sh "$ARCH" "$APPDIR/usr/bin/licenses"

# libraries: what the runtime needs (DT_NEEDED, recursively) that deps/ provides; anything else must be a system one
needed() { readelf -d "$1" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'; }
SYSTEM_LIBS='^(libc|libm|libdl|libpthread|librt|libresolv|libstdc\+\+|libgcc_s|libvulkan)\.so|^ld-linux'
todo=("$APPDIR/usr/bin/$EXE_NAME")
while [ ${#todo[@]} -gt 0 ]; do
    f=${todo[0]}; todo=("${todo[@]:1}")
    for n in $(needed "$f"); do
        [ -e "$APPDIR/usr/lib/$n" ] && continue
        if [ -e "$P/lib/$n" ]; then
            cp -L "$P/lib/$n" "$APPDIR/usr/lib/$n"
            patchelf --set-rpath '$ORIGIN' "$APPDIR/usr/lib/$n"
            todo+=("$APPDIR/usr/lib/$n")
        elif ! [[ $n =~ $SYSTEM_LIBS ]]; then
            echo "package: $(basename "$f") needs $n, neither bundled nor a base system library"; exit 1
        fi
    done
done
patchelf --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/$EXE_NAME"
if ls "$APPDIR/usr/lib" | grep -iE 'x264|x265|xvid|postproc|vulkan'; then  # (no GPL codecs, no Vulkan loader)
    echo "package: library that must not ship"; exit 1
fi
ls "$APPDIR/usr/lib" | grep -q '^libswresample\.so' || { echo "package: libswresample is not bundled (the Opus decoder needs it)"; exit 1; }
! find "$APPDIR" -iname '*.nso' -o -iname '*.nca' -o -iname '*.nsp' -o -iname '*.xci' -o -iname '*.keys' -o -iname 'romfs*' \
    -o -iname 'exefs' -o -iname 'lasgame.*' -o -iname 'generated' | grep .  # (nothing of the game)
echo "bundled: $(ls "$APPDIR/usr/lib" | tr '\n' ' ')"

# ---- AppImage (appimagetool runs extracted: no FUSE in the container)
fetch() {  # url sha256 file
    [ -f "build/linux/src/$3" ] || { curl -fsSL -o "build/linux/src/$3.part" "$1" && mv "build/linux/src/$3.part" "build/linux/src/$3"; }
    echo "$2  build/linux/src/$3" | sha256sum -c --quiet
}
# (appimagetool for the machine this runs on - the container's arch, not necessarily the AppImage's)
HOST=$(uname -m)
case $HOST in
    x86_64) fetch https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
                  ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 appimagetool-x86_64.AppImage ;;
    aarch64) fetch https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-aarch64.AppImage \
                   f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158 appimagetool-aarch64.AppImage ;;
    *) echo "no appimagetool for $HOST"; exit 1 ;;
esac
case $ARCH in
    x86_64) fetch https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
                  2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d runtime-x86_64 ;;
    aarch64) fetch https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-aarch64 \
                   00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444 runtime-aarch64 ;;
esac
chmod +x "build/linux/src/appimagetool-$HOST.AppImage"
mkdir -p build/dist && rm -f "$OUT"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$ARCH "build/linux/src/appimagetool-$HOST.AppImage" --no-appstream \
    --runtime-file "build/linux/src/runtime-$ARCH" "$APPDIR" "$OUT"
ls -la "$OUT"
