# Upstream: LAS Recomp

The runtime engine, the game-code generator and the build come from LAS Recomp (https://github.com/peiikai/LASRecomp,
MIT, see `LICENSE-LASRecomp`), copied at one commit. They are not merged: a later LAS fix is brought over by hand,
file by file.

Commit: 33784c2ad6ce7f802eb4b78139fb578633e60aa4 (LAS `main`, 2026-11-15)

## What was copied

`tools/upstream/paths.txt` lists the LAS paths this repo takes: `Makefile`, `runtime/`, `tools/`, `third_party/`.
`tools/upstream/removed.txt` lists what is left out of them: Link's Awakening's features (randomizer, cosmetics,
weather, warps, cheats, trackers and the rest) and its game data (addresses, RomFS file hashes, file cuts, hot
functions, shader variants, patch and hook sites). LAS's docs, README, CI workflows and Android project are not taken.
`tools/upstream/import.sh <LAS checkout> <commit>` made the copy. Every copied file keeps LAS's notices as they are.

## Changed here after copying

`tools/upstream/sync.sh <LAS checkout> --local` prints this list from the files themselves. Each change removes a
Link's Awakening feature or its data, takes a game value from `game/game.h` / `game/game.mk` (the game layer), or
carries one of this repo's fixes; the engine stays as LAS has it otherwise.

- `Makefile`: no randomizer helper, venv or smoke test; the generator helper's venv is `build/gen-venv`. Includes
  `game/game.mk` (exe name, version); `make gen` lifts every module `game/game.h` lists (`GAME=` names the game
  files) with the Python in `.venv`; the game code links through a linker response file. `WORK=` puts the game
  files, the generated C and the build in a work folder. Includes `tools/dist.mk` (the Windows release, in place of
  LAS's `dist`), `tools/ci/ci.mk` and `tools/verify/verify.mk`; `make test-setup-cli`.
- `runtime/settings.c`, `runtime/settings.h`: no settings of the removed features.
- `runtime/main.c`: no seed at boot; `LARECOMP_REBOOT_AT` takes `game` or `title`. nn::init::Start's symbol and
  argument count come from `game.h` (5 arguments: every module initialised at x3, the SDK's first).
- `runtime/game_addresses.c`, `runtime/game_addresses.h`: no version tables of their game; the address table has a
  placeholder field (C has no empty struct).
- `runtime/gamelib.h`, `gamelib.c`, `dispatch.c`, `loader.c`, `rt.h`, `gamebuild.c`, `tools/gamelib_entry.c`: any
  number of modules (`game.h` GAME_MODULES; GAMELIB_ABI 2), where LAS has main and sdk.
- `runtime/kernel.c`: the program id from `game.h`; `kernel_run_main` passes x4; a condition variable wait sets its
  key before it releases the mutex (`tools/test_condvar.c` has the case); `LARECOMP_LOCKSTEP` keeps the game clock
  within one frame of the last present.
- `runtime/services/misc.c`: the add-on base id from `game.h`.
- `runtime/romfs_build.c`, `romfs_build.h`, `services/fs.c`: `romfs_is_game()` (the marker folders in `game.h`) in
  place of the `region_common` checks.
- `runtime/paths.c`, `runtime/platform/windows.c`: message boxes named after the game; Windows: the session log is
  `game.log` (`game.h` GAME_LOG).
- `runtime/nvn/objects.c`: a packaged 3D texture's layout word gives its block depth too (bits 4-7).
- `runtime/nvn/maxwell/translate.c`, `shader.h`: geometry shaders (ALD by vertex, ISBERD, OUT, VMAD, VSETP), SUST.P,
  TMML, RED and atomics on global memory, LDS and STS, a BRX jump table read across a forward BRA, integer textures
  sampled as integers. VOTE.VTG is a no-op, EXIT honours its flow test (EXIT.FCSM_TR goes on), LEA's
  lo forms; none of the patches that matched LA's shaders (light-culling tiles, the ambient occlusion's pixel grid
  and blur taps and its Off pass, the shadow map's tap offset), nor the `rscale[65]` slots and `uses_ao_taps` that fed
  them (`shader.h`, `vulkan/exec.c`, `renderer.m`, `metal.m`, `vulkan/textures.c`).
- `tools/game_addresses.py`: an empty address table is allowed. `tools/gen.py`: an ADR whose register a BR uses soon
  after is a jump table, not a function start; a function over 200 KB of C is compiled without optimization.
- `tools/zig_slim.sh`, `tools/frames.sh`, `tools/game_slot.sh`: the game's exe name and `tools/dist.mk`'s layout.
  `tools/test_gamebuild_resume.py`, `test_windows_logs.py`, `test_movie_extractor.c`, `test_spirv_val.sh`,
  `test_translate.c`, `test_translate_flow.c`, `test_translate_ops.c`: cases for the changes above (three modules,
  `GAME_LOG`, geometry shaders, the new instructions).
- `runtime/hle/input.c`: no controller light (it showed Link's health), d-pad items, mirror mode, warps or test hooks
  of the removed features; `g_window_shot_request` is defined here (it was in `hle/hookshot.c`). Input scripts can
  count frames from a game event; `LARECOMP_VPAD` drives an SDL virtual gamepad.
- `runtime/hle/movie.c`: no run-timer calls. Extractor slot 2 SetDataSource(path), MediaData::FindInt64, decoded
  frames in tight NV12 and copied into a frame buffer the game passes.
- `runtime/services/fs.c`: the RomFS overlay has no source (it served randomizer seeds); no per-seed save folder or
  save hooks of the removed features. Save file reads and writes are logged; `LARECOMP_STAGE` (with `hle/libc.c`).
- `runtime/nvn/pace.c`: no "30 fps outdoors" by level, no frame-work object (it came from their game's addresses).
- `runtime/nvn/vulkan/exec.c`, `device.c`, `shaders.c`, `textures.c`: no weather, snow, mirror mode, recolouring,
  texture patches, screen filter, phone HUD corners, pause backdrop or room widening; the present crop is the whole
  image and the wide view is off. `device.c`: the window title from `game.h`.
- `runtime/nvn/vulkan/exec.c`, `device.c`, `shaders.c`, `textures.c`, `record.c`, `vk.h`: the frame hold (with no
  notice over it: LAS's own ui/menu.mm notice box and its strings removed), geometry-stage
  pipelines, storage images for SUST, only the device features the GPU reports, integer textures without filtering,
  a feedback snapshot reused only for its own format and size, the pipeline cache saved after a burst of builds, the
  log's GPU ids and features, validation message counts, and a lost GPU's faulting address named.
- `runtime/crash_explain.c`: a comment points at `hle/game_state.c`; no RomFS file count (the incomplete-copy
  explanation is off); names the app and the launcher, a lost GPU, and repeats the fatal line after the trace
  (`tools/test_crash_explain.c`).
- `runtime/nvn/cmdbuf.c`, `nvn.h`: a command list longer than the game's command memory goes on in host memory
  (`tools/test_cmdbuf.c`).
- `runtime/nvn/shader_prep.c`: prepares the player's own list when the shipped one is empty.
- `runtime/ipc.c`, `ipc.h`, `main.c`: `sm` refuses services outside the game's access list (NotAllowed). `main.c` also
  starts the launchers' commands (`setup_cli.c`).
- `runtime/services/am.c`: the commands the game sends at boot. `runtime/services/misc.c`: `prepo:u`, `bcat:u` and
  `ectx:aw` answer as an offline console.
- `runtime/services/audio.c`: the Delay and I3DL2 reverb effects (`services/audio_fx.h`, adapted from Ryujinx, MIT,
  `third_party/ryujinx/LICENSE.txt`).
- `runtime/gamebuild.c`, `runtime/build_cache.cpp`: any module of the game; the biggest file compiles first; the build
  log names each file's compile time and the progress names a slow file. `runtime/hle/pl.c`, `paths.c`,
  `settings.c`, `platform/windows.c`: the app's name (`GAME_NAME`) in the log, message boxes and errors.
- `runtime/ui/menu.mm`, `launcher.mm`, `controls_ui.mm`, `difficulty.mm`, `option_rows.mm`, `option_rows.h`: no
  Randomizer, Warp, Save Editor, Cheats, Cosmetics or Gameplay pages, overlays or rows; the start screen has no
  Randomizer row. `launcher.mm`: version, update check repository, picker text and RomFS check from the game layer;
  no RomFS file count.
- `tools/gen.py`, `tools/lasgen.spec`: no randomizer patch variants.
- `tools/menu_strings.py`: only the menus' own text (no warp, spoiler or tracker names).
- `tools/build_deps_windows.sh`: FFmpeg keeps swresample (its Opus decoder needs it, for `services/hwopus.c`) and the
  build checks that every decoder was built.
- `tools/test_settings.c`, `tools/test_settings.py`: no cases for the removed settings.

## This repo's own files among them

- The game layer, outside the copied paths: `game/game.h` (name, program id, log name, RomFS markers, modules,
  nn::init::Start) and `game/game.mk` (exe name, version).
- The game's slots: `runtime/game_addresses/versions.h` (Kirby 1.0.0's main build ID, its one line); empty:
  `patches_v1_0_0.h`, `runtime_v1_0_0.h`, `romfs_files.h`, `runtime/hle/hooks.c`, `runtime/nvn/shader_variants.txt`,
  `tools/patches.py`, `tools/chunk_bounds.py`, and `runtime/menu_sfx.c` (silent).
- `runtime/services/hwopus.c` (Opus decoding) and `network.c` (an offline console's nifm, ssl, bsd, ldn), from
  letsgo-recomp (MIT) by way of v0.2.0.
- `runtime/setup_cli.c`, `setup_cli.h`: the launchers' commands (`docs/launcher-runtime-contract.md`).
  `runtime/hle/stage_jump.c`: `LARECOMP_STAGE`. `runtime/services/audio_fx.h`: the audio effects.
- `runtime/hle/game_state.c`: what the engine asks the game (in play, level name, world on screen, camera zone, RomFS
  read hook), answered neutrally, and the `__cxa_throw` and `vector::at` diagnostics moved out of `hle/warp.c`.
- `tools/upstream/`: the copy's lists and scripts. `tools/ci/` and `tools/verify/`: the CI suites and the checks
  that need the game (`docs/testing.md`).
- The test harness and packaging scripts in `tools/` that LAS doesn't have (`run_scripted.sh`, `boot_check.sh`,
  `test_find_game.sh`, `dist_licenses.sh`, `dist.mk`, `launcher_notice.py`, `check_game_identity.py`,
  `audio_check.py`, `input/`, `frametime_summary.py`, `perf_summary.py`, `winshot.ps1`, `test_audio_fx.c`,
  `test_cmdbuf.c`, `test_setup_cli.sh`, `test_check_game_identity.py`).

## Bringing a LAS fix over

1. `tools/upstream/sync.sh <LAS checkout> [rev]` lists every taken file LAS changed since the commit above, and says
   for each whether this repo changed it too.
2. Copy the files unchanged here; merge the others by hand; put a new LAS file in `removed.txt` if it belongs to their
   game.
3. Set the Commit line above to the new commit, and update the lists in this file.
