#!/bin/bash
# make appimage ARCH=<x86_64|aarch64>: build/dist/ForgottenLandRecomp-<version>-Linux-<arch>.AppImage, built in Ubuntu 22.04
# containers (podman or docker; tools/linux/Containerfile) so it runs on glibc 2.35+ distributions. On a host of the same
# arch everything is built natively in that arch's container. An aarch64 AppImage on an x86-64 host: the amd64 container
# cross-compiles against the arm64 image's root filesystem (build/linux/aarch64/sysroot), only the generator helper is
# frozen in the arm64 container (PyInstaller builds for the machine it runs on: QEMU, binfmt - slow, since pip also
# compiles packages there). No game code inside: the setup builds it on the player's machine. See docs/linux.md.
# CONTAINER=docker|podman picks the tool (default: podman when installed).
set -euo pipefail
cd "$(dirname "$0")/../.."
ARCH=${1:?usage: tools/linux/appimage.sh x86_64|aarch64}
CT=${CONTAINER:-$(command -v podman >/dev/null && echo podman || echo docker)}
case $ARCH in x86_64) PODARCH=amd64 ;; aarch64) PODARCH=arm64 ;;
    *) echo "unknown arch $ARCH"; exit 1 ;; esac

image() {  # arch -> tag (built once, then cached by the container tool)
    $CT build -q --platform "linux/$1" -t "localhost/forgottenland-build:$1" -f tools/linux/Containerfile tools/linux >/dev/null || return
    echo "localhost/forgottenland-build:$1"
}
run() {  # image, command...: the checkout mounted at /src
    local img=$1; shift
    $CT run --rm --platform "linux/${img##*:}" -v "$PWD:/src" -w /src -e SYSROOT=/src/build/linux/aarch64/sysroot "$img" "$@"
}

if [ "$ARCH" = "$(uname -m)" ]; then  # native: one container of this arch does it all
    IMG=$(image "$PODARCH")
    run "$IMG" tools/linux/gen_helper.sh "$ARCH"
    run "$IMG" tools/linux/package.sh "$ARCH"
    exit 0
fi
AMD=$(image amd64)
if [ "$ARCH" = aarch64 ]; then
    ARM=$(image arm64)
    S=build/linux/aarch64/sysroot
    if [ ! -f "$S/.ok" ]; then  # the arm64 image's files: headers and libraries to cross-compile against
        rm -rf "$S" && mkdir -p "$S"
        c=$($CT create --platform linux/arm64 "$ARM")
        # (anchored: only the top-level pseudo filesystems, not e.g. usr/include/<triple>/sys)
        $CT export "$c" | tar -x -C "$S" --anchored --exclude='dev/*' --exclude='proc/*' --exclude='sys/*' 2>/dev/null || true
        $CT rm -f "$c" >/dev/null
        # absolute symlinks (libm.so -> /lib/aarch64-linux-gnu/libm.so.6) would point into this machine's root:
        # relative, so they stay inside the sysroot
        find "$S" -type l -lname '/*' | while read -r l; do
            ln -sfn "$(realpath -m --relative-to="$(dirname "$l")" "$S$(readlink "$l")")" "$l"
        done
        touch "$S/.ok"
    fi
    run "$ARM" tools/linux/gen_helper.sh aarch64
else
    run "$AMD" tools/linux/gen_helper.sh x86_64
fi
run "$AMD" tools/linux/package.sh "$ARCH"
