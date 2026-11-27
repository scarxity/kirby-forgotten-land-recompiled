# Linux

The Linux build is **untested by play**: it is built and checked by scripts and in a container, and nobody has played it
on Linux yet. Feedback is welcome (issue #8 is where it started).

## Playing

The release is an AppImage, `ForgottenLandRecomp-<version>-Linux-x86_64.AppImage` (x86-64 with AVX2, glibc 2.35+, a
Vulkan 1.3 driver). It has no window of its own to set the game up, so the setup is a command and playing is the
AppImage itself:

```sh
chmod +x ForgottenLandRecomp-*.AppImage
./ForgottenLandRecomp-*.AppImage --prepare /path/to/the/folder/with/exefs/and/romfs   # once: game code, then shaders
./ForgottenLandRecomp-*.AppImage                                                      # play
```

Everything it writes goes to `~/.local/share/ForgottenLandRecomp` (`$XDG_DATA_HOME`, or `LARECOMP_DATA_ROOT`). Started
with no arguments and nothing set up, it says so (in a window when zenity is installed). Other arguments reach the
runtime as they are: `--check <folder>`, `--status`, `--rebuild-code` (docs/launcher-runtime-contract.md).

How a player starts it, and what each way needs:

| Way | State |
|---|---|
| The AppImage's command line (above) | Works with the runtime's own setup, the one RecompLauncher uses. This is the way the release documents. |
| [RecompLauncher](https://recomplauncher.com) | Its schema lists Linux, unpacks an AppImage and runs its `AppRun`. `recomplauncher.json` names the AppImage next to the Windows exe. Untested here. |
| The Tauri launcher (`launcher/`) | Windows only for now: the exe names (`.exe`, `lasgame.dll`, `explorer`, `taskkill`), the NTFS junction it links the game folder with, the kill-on-close job object and the Windows zip the updater looks for. A Linux port needs those replaced and webkit2gtk. |
| The runtime's own start screen (no arguments, `runtime/ui/launcher.mm`) | LAS Recomp's, with its name and its Zelda texts: not shown on Linux (`AppRun` never reaches it). |

## Building

`tools/linux/appimage.sh x86_64` (`make appimage`) builds it in an Ubuntu 22.04 container (`tools/linux/Containerfile`,
podman or docker, `CONTAINER=docker`): SDL3, an LGPL FFmpeg with only the decoders the game needs, and shaderc from
pinned sources (`tools/linux/build_deps.sh`), the game code generator frozen with PyInstaller
(`tools/linux/gen_helper.sh`), then `tools/linux/package.sh`. The game's own code is not in it: the setup builds that on
the player's machine with the bundled zig.

The Linux flags came from saschb2b's report in issue #8: `-DLARECOMP_ZSTD` and `-l:libzstd.a` (without them the shader
preparation finds 106 of the game's 16,963 shaders, the rest compile during play), and FFmpeg built with swresample
(without it configure drops the Opus decoder without an error, and the game stops where a level's music starts).
`build_deps.sh` now checks that every decoder the game needs is in the built library.

## What CI checks

The `linux` job in `.github/workflows/ci.yml`: the container image, the dependencies, the runtime build, the game-free
tests that run on Linux (`tools/ci/run.sh linux`), and the AppImage, which it keeps as a workflow artifact for seven days
for testing. The tests that need `glslc` or `spirv-val`, and the Windows-only ones, are listed as SKIP in its summary.
