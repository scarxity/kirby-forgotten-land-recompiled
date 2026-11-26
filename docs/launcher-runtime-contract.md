# What the launcher expects from the runtime

The Tauri launcher (`launcher/`) and RecompLauncher (`recomplauncher.json`) drive `ForgottenLandRecomp.exe` from outside.
This page lists everything they rely on, so the runtime can change without breaking them. The values that name the game
(program ids, folder names, file counts) come from `game/game.h`. The launcher keeps its own copy of them, checked by
`tools/check_game_identity.py`.

The runtime side is `runtime/setup_cli.c` (the commands), `runtime/romfs_build.c` (the search for the game's files, the
copy and the RomFS checks) and `runtime/gamebuild.c` (the game code). The search is tested by
`tools/test_find_game.sh`. When this page and `setup_cli.c` disagree, `setup_cli.c` is right and this page is the bug.

## Process rules for every command below

- Started with the working directory set to the data root and `LARECOMP_DATA_ROOT=<data root>`. The data root is the
  install folder in the release zip. Without that variable `--prepare` must use the exe's own folder.
- The release exe is a GUI-subsystem program. Output the launcher reads goes to **stdout** as whole lines, flushed
  after each one. The launcher starts these commands with stdout piped, stdin null, and stderr either null
  (`--check`, `--status`) or the file `setup.log` (`--setup`, `--rebuild-code`). LAS's `plat_cli_*` channel writes to
  stderr, so it is not the channel for these lines. Lines that are not in the formats below are ignored, but a line
  that starts with `phase`, `done`, `ok` or `failed` is always read as a protocol line.
- `--check` and `--status` change nothing, open no window, write no session log (`game.log` and
  `game-backup-*.log` belong to a game run) and answer in well under a second on a folder of thousands of files.
- A command's stderr is free text for people. It is never parsed.

## Commands

| Command | stdout | Exit code |
|---|---|---|
| `extracted --play` | none (see "A game run") | see "A game run" |
| `--check <folder>` | one line, `ok` or `failed <reason>` | 0 for `ok`, 1 for `failed` |
| `--status` | one line, `ready`, `build` or `none` | 0 |
| `--shaders-status` | one line, `ready` or `prepare <what changed>` | 0 |
| `--setup <folder>` | `phase <name> <0..1> [(<slow file>, <n> min)]` lines, then `done` or `failed <reason>` | 0 for `done`, 1 for `failed` |
| `--rebuild-code` | the same lines as `--setup` | the same |
| `--prepare <folder>` | `progress <0..1> <text>` lines, ending with `progress 1 Ready` | 0, or the failure code of the table below |
| `extracted --build-romfs` | none | 0 when `extracted` holds a RomFS (see `--status`), else 1 and `romfs image: no game RomFS (a romfs folder or a romfs.bin image) in extracted` on stderr |

`--build-romfs` builds nothing: the game is served from the RomFS folder, so there is no image to write. It only checks
that a RomFS is there, and only the launcher's developer mode runs it, to check the dump it linked as `extracted`.

### `--check <folder>`
The setup's search and program id check, without copying. A folder holds the game when the search finds:
- an `exefs` folder (any name) that holds `main` and `main.npdm`;
- a RomFS: a folder holding every name of `GAME_ROMFS_MARKERS` (`msg` and `gfx`), or a `romfs.bin` image file;
- a program id in `main.npdm` equal to `GAME_PROGRAM_ID`. The id is at `ACI0 + 0x10`, where the `ACI0` offset is the
  32-bit value at `META + 0x70`;
- a `main` whose build id is a supported version's (`game_release_label`; the supported one is 1.0.0).

The search looks in the picked folder and up to four levels below it (subfolders in name order, names that start with
`.` skipped) and takes the first exefs, the first RomFS folder and the first `romfs.bin` it meets. It stops looking
inside a folder that is itself the RomFS. It does not read the RomFS folder or the image: `--check` never answers
`bad_image`.

Reasons, in the order they are decided:

| Reason | When |
|---|---|
| `not_found` | no exefs, or neither a RomFS folder nor a `romfs.bin`. Also when the exefs found is inside the data root's own `extracted` folder |
| `package` | as `not_found` (without the extracted-folder case), and the search met a `.xci`, `.nsp`, `.nca`, `.nsz` or `.xcz` file |
| `update` | the program id is the update's: `GAME_UPDATE_PROGRAM_ID` in `game/game.h` (`GAME_PROGRAM_ID + 0x800`, `01004D300C5AE800`) |
| `version` | any other program id, an unreadable `main.npdm`, or a `main` that is not a supported version's NSO |

`tools/test_find_game.sh` has a case for `update` (a `main.npdm` with `...E800`).
The launcher and `--setup` must use one search, so they cannot disagree about a folder (0.2.0 fixed exactly that).
`--check` looks at the program id only: a folder with the base game's id but a version this release does not support
passes it, and `--setup` then answers `version` (see below).

### `--status`
What is set up in the data root:
- `ready`: `extracted/exefs` has the game's program id; the RomFS is set up, meaning `extracted/romfs` is the game's
  RomFS (it holds the `GAME_ROMFS_MARKERS` folders `msg` and `gfx`) or `extracted/romfs.bin` is a readable image
  (`romfs_image_ok`); and `extracted/lasgame.dll` exists with a `lasgame.dll.id` equal to this runtime's id for it
  (the library id and the game's build id, `gamelib_full_id`).
- `build`: the same exefs and RomFS, but the library is missing or its id differs. The game code needs building again
  from the files already in `extracted` (a new version unzipped over an old folder). `--rebuild-code` does it.
- `none`: anything else.

The launcher runs it once per change of the modification times of what it reads: the runtime exe,
`extracted/lasgame.dll`, `extracted/lasgame.dll.id`, the folder `extracted/romfs` and `extracted/romfs.bin`. So the
setup must write `lasgame.dll.id` last, after `lasgame.dll`.

### `--shaders-status`
Whether the shaders were prepared for this exact runtime, so Play can start at once. It is the check `--play` makes
(below), so a launcher that asks and the game that checks cannot disagree: **the launcher keeps no rule of its own**.
It prints `ready`, or `prepare <field>` where `<field>` is the first thing that changed: `none` (no stamp: never
prepared), `runtime` (version, shader translator, cache format), `exe` (the runtime file's size or time), `gpu` (the
GPU or its driver version: the pipeline cache's own key), `list` (the shipped variant list), `code` (the game code
library: id, size, time) or `mods` (the mods switched on, in order). Unlike `--status` it loads the GPU driver (a
Vulkan instance, no window), about a second at most, so the Tauri launcher asks again only when a file it reads changes
(the runtime, the game code, the stamp, the mods).

The stamp is `cache/shaders-prepared.txt` in the data root (`runtime/shader_stamp.h`): one `<field> <value>` line per
field above. **Only the game writes it**, at the end of a full preparation (every program of the game files, not
cancelled, the caches saved); a launcher never writes it. Wiping `cache/` wipes it with the caches it vouches for.

### `--setup <folder>` and `--rebuild-code`
`--setup` finds the game as `--check` does, with the same verdict. Before it removes anything it refuses a RomFS it
cannot read (`bad_image`). Then it replaces
what an earlier setup made in `<data root>/extracted` (`exefs`, `romfs`, `romfs.bin`, the library and its id; never the
folder the player picked, never a link, nothing outside `<data root>/extracted`, and not `.gamebuild`), copies `exefs`,
sets up the RomFS and builds the game code. The RomFS is set up in one of two ways:
- the picked folder is copied into `extracted/romfs/`, file by file;
- when the search found a `romfs.bin` image and no RomFS folder, that one file is copied to `extracted/romfs.bin` and no
  `extracted/romfs/` is made.

Both use a hard link where the file system allows (the same NTFS volume), else a copy, so on the same drive the game's
data takes no extra space and on another drive about 6 GB. No image is ever built.
Killing it at any point is safe: the next run starts over, and the build reuses what `.gamebuild` still holds (a
cancelled build resumes, an update compiles only the files that changed). `--rebuild-code` does only the build, from
the files in `extracted` (no folder argument; it reads the exefs there, and answers `not_found` when `--status` would
say `none`).

Phase names in `phase` lines, the fraction of the whole setup (copying is the first 15%, the build the rest):

| Name | Meaning | Fraction |
|---|---|---|
| `looking` | searching the folder | 0 |
| `copying` | copying exefs and the RomFS | 0 to 0.15 (progress by bytes while a folder is copied) |
| `reading` | the build is preparing, checking its earlier work, and the generator is lifting the NSOs | from 0.15 |
| `compiling` | compiling the game code (`Recompiling...`, or `Resuming compilation...` after a cancelled build) | up to 1 |
| `finishing` | linking (`Finishing...`) | about 0.96 and over |

A line is printed only when the phase, the fraction (in thousandths) or the slow-file text changed. The slow-file text
is `(<file>, <n> min)`, added once one file has taken more than a minute (the file the compiler has been on longest),
and the launcher shows it as it is. The phase comes from the build's step text: one that begins `Finishing` is
`finishing`, `Recompiling` or `Resuming` is `compiling`, anything else `reading`. `--rebuild-code` starts at `reading`.

Failure reasons after `failed`: the `--check` ones, and

| Reason | Meaning |
|---|---|
| `bad_image` | the picked RomFS cannot be read: a folder whose tree cannot be walked, or a `romfs.bin` that is not a plain RomFS (an IVFC image, say) or is cut short (a file's data ends past the end of the image). There is no count of the files |
| `copy` | the files could not be copied (disk full, unreadable), or the earlier setup could not be removed |
| `build` | the game code did not build (or the build left no usable library) |
| `tools` | the generator or the compiler is missing from this install |

### `--prepare <folder>` (RecompLauncher)
The same setup as `--setup`, with the output RecompLauncher parses: `progress <0..1> <text>` where the text is one of
`Looking for the game files`, `Copying the game files`, `Reading the game code`, `Compiling the game code` and
`Finishing`, followed by the slow-file text `(<file>, <n> min)` when there is one, then `Preparing shaders` (the
Tauri launcher's shader step, run as a hidden child `extracted --play`: the game code is 15-70% of the bar, the shaders
70-100%; a failed shader step still ends in Ready, and the shaders are then built during play). Success ends with `progress 1 Ready`
and exit 0. Without `LARECOMP_DATA_ROOT` the data root is the folder of the exe, wherever it was started from, and the
folder argument is resolved before the working directory changes. A failure exits with its code and prints nothing more:

| Code | Reason | Code | Reason |
|---|---|---|---|
| 8 | `not_found` | 12 | `build` |
| 9 | `package` | 13 | `tools` |
| 10 | `version` | 14 | `bad_image` |
| 11 | `copy` | 15 | `update` |

The codes are the enum in `runtime/setup_cli.c` (`NOT_FOUND` = 8, in the order of this table). `recomplauncher.json`
`exit_codes` words each one for the player: it must have all eight.

## A game run
Both launchers start `ForgottenLandRecomp.exe extracted --play` (RecompLauncher adds nothing; the Tauri launcher
adds the environment below). `--play` goes straight to the game, never the LAS start screen.

**The game prepares its own shaders.** Before it boots, `--play` makes the `--shaders-status` check. When the stamp does
not match (a new release or runtime, a game code rebuild, a new shipped list, a mod switched, a GPU driver update, or
never prepared), the game opens its window with a progress bar ("Preparing shaders", with Skip) over the title picture,
prepares every shader of the game files (`[shader] preparing: N%` lines in the log, every 5%), writes the stamp, and
boots in that same window. So a play started by RecompLauncher's start command, a shortcut or the launcher never plays with
unprepared shaders; the launcher's and `--prepare`'s own "Preparing shaders" step only makes it happen earlier, in
their window. Skip leaves the stamp unwritten, so the next start asks again. Not run: a test run (`LARECOMP_NO_LAUNCHER`
or `LARECOMP_INPUT` set), and the preparation run itself (`LARECOMP_SHADER_PREP=1`, the hidden child of the Tauri
launcher and of `--prepare`).

- Working directory and `LARECOMP_DATA_ROOT`: the data root.
- `LARECOMP_GAME_LIB=<data root>\extracted\lasgame.dll` (Tauri launcher; RecompLauncher relies on the default place).
  The runtime loads that file as given and refuses a library whose id is not its own.
- The Tauri launcher truncates `game.log` in the data root and gives it as stdout and stderr, so the runtime must keep
  an inherited file or pipe as it is and not start a log of its own (LAS's `log_to_file_without_console` already does:
  windows.c, the `fresh && !g_cli` test). Started on its own, the runtime logs to `game.log` with
  `game-backup-*.log` copies (`GAME_LOG` in `game/game.h`), and the README and the launcher name `game.log`.
- The launcher puts each setup step it starts (`--setup`, `--rebuild-code`, the developer mode's make) in a
  kill-on-close job object, so closing the launcher ends a build and the compilers under it. The game it started is
  not in that job and keeps running.

Exit code and crash report, read by the launcher after the process ends (`classify` in `launcher/src-tauri/src/main.rs`):
- 0 is a normal close.
- A line of `game.log` that starts (after trimming) with `FATAL:` or `*** exception` is a crash, and that line is shown
  to the player (300 characters). `rt_fatal_msg` prints the first, `rt_fault_report` the second, then the process
  aborts, which exits with code 3. Code 3 without such a line is "aborted without a report".
- An exit code from 0xC0000000 up (an NTSTATUS) is "Windows ended the game with error ...".
- When the crash line holds `Vulkan` or `SDL_CreateWindow`, the player is told to update the graphics driver and the
  line follows in parentheses. A start-up failure that wants that advice must say one of those words.
- Any run that ends within 15 seconds of its start is reported as "closed N seconds after it started", crash line or not.
- The launcher copies `game.log` to `crash-<yyyymmdd>-<hhmmss>.log` for every reported run and keeps the last five.

## Files and names

In the data root (the install folder):

| Path | Who | What |
|---|---|---|
| `extracted\exefs\` | setup | `main`, `main.npdm`, `sdk`, `subsdk0`, and `rtld` when the dump has it |
| `extracted\romfs\` | setup | the game's RomFS folder (`msg`, `gfx` and the rest), about 6 GB: hard links to the picked folder's files where possible |
| `extracted\romfs.bin` | setup | only when the player picked a `romfs.bin` image instead of a RomFS folder: that file (a hard link or a copy), about 6 GB |
| `extracted\lasgame.dll`, `extracted\lasgame.dll.id` | setup | the game code and the id `--status` compares |
| `extracted\.gamebuild\` | setup | the generated C and the compiler's files. Kept after the build, so the next one compiles only what changed and a cancelled one resumes; `--setup` does not remove it |
| `larecomp.ini` | runtime, launcher | settings, `key = value` lines (`SETTINGS_FILE` in `runtime/settings.c`) |
| `save\` | runtime | saves (`save\save_0\file0` and the rest) |
| `cache\` | runtime | shader and pipeline caches |
| `mods\<mod>\romfs\<path>` | player | a mod: this file replaces the game's RomFS file `<path>`. A folder is a mod when it holds `romfs\`; names starting with `.` are skipped |
| `mods\mods.txt` | launcher | the mods in priority order, one name a line, `-<name>` for one switched off |
| `game.log` | launcher, or the runtime on its own | the log of the last run |
| `setup.log` | launcher | stdout and stderr of the last Build |
| `crash-*.log` | launcher | copies of `game.log` after crashes |
| `game-folder.txt` | launcher | the folder the player picked, until the build reads it |

The setup only ever deletes entries it made in `extracted` (`--setup` removes `exefs`, `romfs`, `romfs.bin`, the library
and its id, by `rt_remove_game_file`, which refuses any name outside its list in `runtime/paths.c`) and refuses when
`extracted` is a link or not `<data root>\extracted`. The launcher's developer mode links `extracted` to the player's
dump on purpose, so a runtime started by it must never run `--setup`.

Mods (`runtime/mods.c`, used by `runtime/services/fs.c`; the launcher's `read_mods` and `write_mods` are the same rule
and must change with it). The runtime reads `mods\` once, when the game starts, and logs `[fs] mod N: <name>` for each mod
switched on and `[fs] mods: K romfs files replaced`. The first mod switched on that has a file wins. The order is
`mods.txt` first (the first mention of a name wins, unknown names are ignored, a `-` before a name is off), then the
mods it does not list, in name order, switched on. The launcher writes the whole order each time, to `mods.txt.tmp`
and renames it over `mods.txt`, and leaves an on mod whose name starts with `-` out (that line would read as off). It
only replaces files: a mod cannot add a file the game's RomFS lacks. The project ships no mod and no file derived from
the game.

Settings the launcher writes and RecompLauncher sets (`recomplauncher.json` `settings`), all of which the runtime's
`settings_load` must read: `fullscreen` (`0` or `1`), `resolution` (`match`, `75%`, `50%`, `25%`, `1x`, `2x`) and
`volume` (`0`..`1`). `tools/check_game_identity.py` checks that each name is in `runtime/settings.c`.

Beside the runtime in the release folder (found with `rt_resource`; the launcher reports each as a missing build
tool when it is absent):

| Path | What |
|---|---|
| `ForgottenLandRecomp.exe` | the runtime (`EXE_NAME` in `game/game.mk`) |
| `lasrecomp.dll.a` | the exe's import library, which the game code links against |
| `lasgen\lasgen.exe` | the frozen generator |
| `zig\zig.exe`, `zig\rt\link.rsp` | the compiler and its prebuilt runtime |
| `gamelib-src\` | `cpu.h`, `simd.h`, `simd_fp.h`, `gamelib.h`, `gamelib_entry.c`, `hle_symbols.txt`, plus every header they include |
| `icon.png`, `*.ttf` | window icon and fonts |

The launcher's own copy of the game's values: `PROGRAM_ID` and `UPDATE_ID` (program ids), `MODULES` (`main`, `sdk`,
`subsdk0`: the exefs files its developer-mode check requires), the RomFS marker folders `msg` and `gfx`, the
runtime's file name, `BUILD_BYTES` (6.5 GB free disk before a build: the worst case, the game's files copied to the drive; the launcher
cannot tell beforehand whether they will be hard-linked, which costs none of it) and `CODE_BYTES` (1 GB, a code-only
rebuild).
`MAX_WORK_PATH` is 200 characters, measured with the bundled zig: the build adds about 45 below the install folder and
Windows stops at 260.

## Developer mode (source checkout)

The launcher beside a checkout (a `Makefile` and `runtime\` above the exe, none of the bundled tools) builds with
MSYS2 `make` and works in `KFL_WORK` (default `..\work\kfl`). It runs, from the repository, with `WORK=<work relative to
the repo>`:

```
make WORK=<w> -j<n> <w>/build/ForgottenLandRecomp.exe
<work>\build\ForgottenLandRecomp.exe extracted --build-romfs        (cwd <work>; only checks the dump has a RomFS)
make WORK=<w> gen
make WORK=<w> -j<n>
```

and counts `*.c` in `<work>\generated` against `*.o` in `<work>\build\obj` for the progress bar. So the Makefile must
take `WORK` (the game files in `$(WORK)\extracted`, the generated C in `$(WORK)\generated`, the build in
`$(WORK)\build`), and keep the object paths short enough to link (a response file does that). It also needs
`.venv\Scripts\python.exe` in the repository. If the LAS Makefile keeps `build/` and `generated/` in the repository,
developer mode does not work, and `Mode::Dev` can be deleted instead: the release folder mode is the one players use.
