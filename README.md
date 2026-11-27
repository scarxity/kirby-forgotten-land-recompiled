# Kirby and the Forgotten Land Recompiled

<a href="https://recomplauncher.com"><picture><source media="(prefers-color-scheme: dark)" srcset="https://recomplauncher.com/assets/press/launcher/badge-light.svg"><img alt="Get it on RecompLauncher" src="https://recomplauncher.com/assets/press/launcher/badge-dark.svg" height="48"></picture></a>
<a href="https://recompstore.com/dev/scarxity"><picture><source media="(prefers-color-scheme: dark)" srcset="https://recompstore.com/assets/press/verified-light.svg"><img alt="Verified developer on RecompStore" src="https://recompstore.com/assets/press/verified-dark.svg" height="48"></picture></a>

A work-in-progress static recompilation of **Kirby and the Forgotten Land** (Nintendo Switch) for Windows.
The game's ARM64 code is translated to C and compiled for your PC, and a runtime stands in for the Switch. It is not
an emulator.

> [!IMPORTANT]
> **No game files are included, and none may be added.** You need your own legally obtained copy, extracted to
> `exefs` and `romfs` folders. This project provides no game files, keys or dumping tools. No game code, assets or
> fonts are included. Only a list of shader variant records ships (`runtime/nvn/shader_variants.txt`), never shader
> code. It is a fan project, not affiliated with Nintendo or HAL Laboratory.

## Status

Version 0.3.0 is rebuilt on [LAS Recomp](https://github.com/peiikai/LASRecomp)'s engine. The game boots, plays its
opening and runs the first stage with scripted input, and a stage tour starts a game at each of its 109 stages. Stage
music plays. The launcher prepares every shader of the game files before the first play, as a PC game does, so play
compiles almost none. [docs/STATUS.md](docs/STATUS.md) lists what has been checked and how, and the known problems.

Supported dump: the base game, v1.0.0 (program id `01004D300C5AE000`, `main` build id `d9ba7db72ffafecd`). The
launcher refuses other versions and the update when you pick the folder.

## Download

Get `ForgottenLandRecomp-0.4.0-Windows-x64.zip` from the
[latest release](https://github.com/scarxity/kirby-forgotten-land-recompiled/releases/latest), unzip it, open
`ForgottenLandLauncher.exe`, pick the folder that holds your `exefs` and `romfs`, then Build and Play. The zip holds
tools only. The game code is built on your PC from your own copy, in about 7 minutes on a 16-thread PC.

You need:

- Windows 10 or 11, 64-bit.
- A CPU with AVX2 (Intel Haswell, 2013, or newer; AMD Excavator or Zen, or newer).
- A graphics driver with Vulkan 1.3.
- 6.5 GB of free disk, which the launcher checks before it builds (1 GB to rebuild only the game code). The setup
  links your `romfs` folder into the install folder when both are on the same drive, which takes no extra space, and
  copies it (about 6 GB) when they are not. It writes no image of the game's data.
- An install path under 200 characters. The launcher checks this too.

The zip's `README.txt` has the controls and what to do when something goes wrong. [CHANGELOG.md](CHANGELOG.md) lists
what changed.

Mods are the player's own: a folder `mods\<name>\romfs\...` in the install folder replaces the game's RomFS files of the
same path, and the launcher's Settings page switches them on and off and sets their order. This project makes and ships
no mod. A 60fps mod by Lhw-Arthas ([#9](https://github.com/scarxity/kirby-forgotten-land-recompiled/pull/9)) is an
example. The zip's `README.txt` has the details.

The zip also carries a `recomplauncher.json`, so [RecompLauncher](https://recomplauncher.com) can set the game up
from the same folder, show its settings and start it.

### Linux

`ForgottenLandRecomp-0.4.0-Linux-x86_64.AppImage` is on the same release page. It is **untested by play**: it builds
and passes the game-free tests in a container, but nobody has played it on Linux yet. Feedback is welcome in
[#8](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/8), where it started.

It needs 64-bit Linux with glibc 2.35 or newer (Ubuntu 22.04 or newer), an x86-64 CPU with AVX2, a Vulkan 1.3 driver
(Mesa RADV or ANV, or NVIDIA's) and 6.5 GB of free disk. The AppImage has no window to set the game up. Set it up once
in a terminal, then start the AppImage to play:

```sh
chmod +x ForgottenLandRecomp-*.AppImage
./ForgottenLandRecomp-*.AppImage --prepare /path/to/the/folder/with/exefs/and/romfs   # once: game code, then shaders
./ForgottenLandRecomp-*.AppImage                                                      # play
```

Everything it writes goes to `~/.local/share/ForgottenLandRecomp`. RecompLauncher can also start it, through the same
`recomplauncher.json` (untested). [docs/linux.md](docs/linux.md) has the details. There is no macOS build.

## Layout

| Path | What it is |
|---|---|
| `game/` | What names this game: `game.h` (program id, folders, modules) and `game.mk` (exe name, version) |
| `runtime/` | The runtime, from LAS Recomp: kernel, services, NVN graphics on Vulkan, audio, input |
| `tools/` | The generator that lifts the game's NSO modules to C (`gen.py`, `lifter.py`), the packaging, the tests and the checks (`ci/`, `verify/`) |
| `launcher/` | The launcher (Tauri 2): pick the game folder, build the game code, play |
| `third_party/` | Bundled libraries, each under its own license |
| `docs/` | Status, testing, the upstream record, what the launchers expect from the runtime |

No game files, game code or assets live in this repository. The one thing recorded from play is the list of shader
variant records, `runtime/nvn/shader_variants.txt`: pipeline state in NVN terms (formats, vertex layout, blend state)
plus each shader's identity as a one-way hash of its code bytes. No shader code ships in any form: the prepare step
finds the code by that hash in the player's own game files and builds the variants on the player's PC, so a first
visit anywhere has no shader pause. The records come from test play and from the records players' games write
(`cache/shader_seen.txt`), merged with `tools/shader_list_merge.py`. Players attach `game.log` or `shader_seen.txt` to
their issues: a bot summarises them and labels the issue "log: new shaders", and a weekly workflow gathers the records
into one pull request to read and merge ([docs/issue-triage.md](docs/issue-triage.md)). The extracted game files, the generated C and
the build output go to a **work folder** outside the repository. Pass it to `make` as `WORK=`. The scripts in `tools/` default to
`../work/kfl`.

## Build from source (Windows)

Install the tools once:

1. [Git for Windows](https://git-scm.com/download/win), Python 3.12, Node.js 20+, Rust (for the launcher).
2. MSYS2 with the UCRT64 packages, in PowerShell:
   ```powershell
   winget install --id MSYS2.MSYS2 -e
   $env:MSYSTEM='UCRT64'; C:\msys64\usr\bin\bash.exe -lc "pacman -S --needed --noconfirm --disable-download-timeout make mingw-w64-ucrt-x86_64-clang mingw-w64-ucrt-x86_64-lld mingw-w64-ucrt-x86_64-sdl3 mingw-w64-ucrt-x86_64-ffmpeg mingw-w64-ucrt-x86_64-vulkan-devel mingw-w64-ucrt-x86_64-shaderc mingw-w64-ucrt-x86_64-spirv-tools mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-zstd mingw-w64-ucrt-x86_64-gdb mingw-w64-ucrt-x86_64-vulkan-memory-allocator mingw-w64-ucrt-x86_64-vulkan-validation-layers"
   ```
   To build the release zip, also install `mingw-w64-ucrt-x86_64-nasm` (`make deps-windows` needs it).
3. The generator's Python environment, in the repository folder (the Makefile looks for `.venv`):
   ```powershell
   python -m venv .venv; .venv\Scripts\python.exe -m pip install capstone==5.0.9 unicorn==2.1.4 lz4==4.4.5
   ```

### With the launcher

```powershell
cd launcher; npm install; npx tauri build
.\src-tauri\target\release\kfl-recomp-launcher.exe
```

Started inside a checkout, the launcher builds with MSYS2 `make` in `..\work\kfl` (`KFL_WORK` changes it). Picking
the game folder links it as the work folder's `extracted`. Press **Build**, then **Play**. Build runs the same `make`
steps as below.

### By hand

In Git Bash, with your `exefs` and `romfs` in `../work/kfl/extracted/`:

```bash
export PATH="/c/msys64/ucrt64/bin:/c/msys64/usr/bin:$PATH"
W=../work/kfl
make WORK=$W -j16 $W/build/ForgottenLandRecomp.exe   # the runtime
make WORK=$W gen                                     # translate the game code to C, in $W/generated
make WORK=$W -j16                                    # compile it into $W/build/lasgame.dll
cd $W && LARECOMP_GAME_LIB=build/lasgame.dll ./build/ForgottenLandRecomp.exe extracted --play
```

`make gen` lifts every module `game/game.h` lists (`main`, `sdk`, `subsdk0`) with the Python in `.venv`. `GAME=`
points it at another dump, `GEN=` and `B=` move the generated C and the build. The game reads the `romfs` folder as it
is, so there is no data image to build.

### Tests

[docs/testing.md](docs/testing.md) explains each check. None of them puts game data in the repository.

```bash
make ci-test                                # build the runtime, then run what CI runs (no game files)
make WORK=$W test-find-game test-setup-cli  # the game-folder search and the launchers' commands, on synthetic dumps
make verify-selftest                        # the self-tests of tools/verify
make WORK=$W verify GAME=$W/extracted       # the checks that need the game; output in $W/verify-out
python tools/check_game_identity.py         # the launchers and READMEs agree with game/game.h
(cd launcher/src-tauri && cargo test --locked) && (cd launcher && node --test)
```

### The release zip

```bash
make deps-windows   # the LGPL FFmpeg the release links
make gen-helper     # the generator, frozen with PyInstaller (Python 3.12 via py -3.12; GEN_PY= picks another)
make zig            # the compiler
make dist           # build/dist/ForgottenLandRecomp-<version>-Windows-x64.zip
```

`tools/dist.mk` says what goes into the zip and checks that nothing of the game does.

## Credits

- [LAS Recomp](https://github.com/peiikai/LASRecomp) by Peiikai (MIT) is the engine this version is built on: the
  runtime (kernel, services, graphics, audio, input) and the game code generator, copied at the commit named in
  [docs/UPSTREAM.md](docs/UPSTREAM.md) with its own copyright notices kept.
- [letsgo-recomp](https://github.com/raulzrrs/letsgo-recomp) by raulzrrs (MIT) is where this port started. Versions
  0.1.0 to 0.2.0 were built on it, and its Opus decoder and offline network services are still here.
- Ryujinx (MIT, [its licence](third_party/ryujinx/LICENSE.txt)): the reverb and delay effects follow its algorithms,
  with two defects fixed, and the shader translator follows its decoding of a few instructions.
- The launcher's layout follows [Recompia](https://github.com/Dittobyte/Recompia)'s. Its code and art are original.
- saschb2b fixed the device loss on AMD's RADV driver
  ([#21](https://github.com/scarxity/kirby-forgotten-land-recompiled/pull/21)) and found the fixes the Linux build
  needed ([#8](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/8)).
- Lhw-Arthas fixed the Chinese language codes
  ([#7](https://github.com/scarxity/kirby-forgotten-land-recompiled/pull/7)).

## License

The code in this repository is under the [MIT license](LICENSE); third-party components keep their own licenses. It
grants no rights to the game.
