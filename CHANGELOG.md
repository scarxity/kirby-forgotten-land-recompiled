# Changelog

Versions are tagged `vX.Y.Z`. Small fixes bump the last number; features and large fixes bump the middle one.

## 0.3.3

**Fewer freezes, and none from shaders on a second play.**
- **Installed with RecompLauncher:** shaders were never prepared, so every new object froze the picture while it was
  built: 20 to 30 fps with freezes of up to 2.4 s in the first stage. RecompLauncher's setup now prepares them too
  ("Preparing shaders", about 10 minutes, once).
- **Entering a new area:** the game hands over its shaders when a stage loads; they are now set up in the background
  from that moment, not at the first frame that draws them. Each pipeline built during play is also remembered, and
  built ahead on every later start. Measured on an RTX 3050 Laptop (Frame Generation off), first stage with its three
  guide clips, about 9 minutes: shader pauses of 50 ms or more went from 13 (the longest 240 ms) to 12 (the longest
  186 ms) on a first play, and to none on a second play.

Kirby runs its 3D at 30 fps, as on the original Switch: 60 fps there is the Switch 2 Edition's, a different program.
The game's menus run at 60.

### Added
- The launcher's Settings page has tabs (Display, Graphics, Audio, Controls, General, Mods, Game) with the in-game
  menu's settings, so they can be set before the game starts. The in-game menu is unchanged; both write `larecomp.ini`.
  Each tab has Restore defaults. Settings can't be changed while the game runs
- Updates inside the launcher: it checks GitHub's releases when it starts, and About and Home show the new version, what
  changed and an **Update** button. "Update automatically" (off by default) installs it by itself. "Only update to
  stable versions" (on by default) leaves out test versions. The download's checksum is checked, and your game files,
  game code, shaders, mods, saves and settings are never replaced. Copies installed by RecompLauncher are updated by
  RecompLauncher, so the launcher doesn't check there
- Game tab: open the saves folder, and "Back up saves now" (the last 5 are kept in `save-backups`)
- "Minimise launcher while playing" (General)

### Changed
- A .nsp or .xci picked as the game says how to extract it (in an emulator set up with your own keys)
- Without a saved setting, the game now starts with the aspect ratio on Original 16:9 and "Allow background inputs"
  off. Saved settings are kept. The launcher's volume goes from 0 to 100% (the in-game menu still goes to 200%)
- The game's log names what each slow frame was spent on

### Known issues
- The first visit to an area can still pause up to about 0.2 s once, for shaders the prepare step can't foresee
- The small guide video at the bottom right in the first stage was reported black on one PC, also with Frame
  Generation off; it freezes the game for 2 to 3 s when it ends there. It plays in every test run
- Frame Generation is still experimental: in a test run it made frame pacing worse (the slowest frame of each 2 s rose
  from 35-40 ms to 50-100 ms)
- Intel GPUs are untested on 0.3.x

## 0.3.2

0.3.1 was published for a few hours and withdrawn: it showed "Preparing graphics for this area... (first visit only)"
in the middle of play, over and over. Its other changes are in this version.

**Shaders are prepared before you play** ([#14](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/14)).
In 0.3.0 every new shader froze the picture until the graphics driver had built it, 20 to 140 ms each, on any PC: a
fast GPU stuttered as much as a slow one. Now the launcher prepares every shader of your game files once, as a PC game
does, after the game code is built: "Preparing shaders", about 7 minutes on an 8-core laptop. It runs again only when
the graphics driver, this program or the mods switched on change, and Play does it first then. Nothing is drawn over the
game and no object is ever left out of a frame. Measured on an RTX 3050 Laptop, first stage, first play after the
prepare step: pauses of 50 ms or more went from 52 (the longest 1.4 s) to 9 (the longest 0.13 s), at the first frame of
a new area or a shader the prepare step can't foresee; later plays have none of these.

The prepared shaders take about 2 GB in the `cache` folder.

Unzip over 0.3.0: the game code from 0.3.0 is used as it is; the first Play prepares the shaders.

### Changed
- No "Preparing graphics for this area... (first visit only)" box over the game any more
- Frame Generation is in the game's menu under Experimental (off by default)
- Quit in the game's menu closes the game, back to the launcher you started it from: no "Return to Launcher", which
  opened a second, older start screen in the game's window. Started on its own, ForgottenLandRecomp.exe opens the
  launcher
- The shader caches are files of their own (`cache/spirv.bin`, compressed, and `cache/translations.bin`) instead of
  tens of thousands of small files; the old `cache/spirv` folder is removed. The graphics driver's own shader cache is
  not used where the driver allows it (`LARECOMP_VK_DRIVER_CACHE=1` uses it): it is capped, and once full it kept
  nothing the prepare step built

### Added
- Mods. A folder `mods\<name>\romfs\...` in the install folder replaces the game's RomFS files of the same path.
  The launcher's Settings page lists the mods, switches each on or off and sets their order (`mods\mods.txt`).
  Changes apply the next time the game starts. No mod is included: they are the player's own, for example a 60fps
  mod by Lhw-Arthas ([#9](https://github.com/scarxity/kirby-forgotten-land-recompiled/pull/9))

### Added (testing)
- `LARECOMP_VK_COLD=<n>` makes every pipeline new to the driver's cache, as on a player's first run (the same number
  in the prepare process and the play process), and `tools/verify/hold_stats.py` sums a run's stutter;
  `docs/testing.md` has the steps
- The stage tour's 2-second stats line counts the draws in held frames

### Known issues
- As 0.3.0, and: Intel GPUs are untested on 0.3.x ([#12](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/12)
  was reported on 0.2.0). Frame Generation (in the game's menu) is still untested
- The small guide video at the bottom right in the first stage was reported black on one PC with Frame Generation on.
  It plays in every test run, with Frame Generation too; if you see it, turning Frame Generation off is worth a try

## 0.3.0

**Stage music plays, and a new area no longer starts on a black screen.** This version is rebuilt on LAS Recomp's
engine, copied at the commit in [docs/UPSTREAM.md](docs/UPSTREAM.md), so later LAS fixes can be brought over instead of
ported by hand. Kirby's part is a small layer next to it (`game/`). 0.2.1 was never released, and its fixes are in
this version.

No game files and no game code are included. The zip holds tools only; the game code is built on your PC from your
own extracted copy (base game, v1.0.0). The game code is generated differently now, so it is built again: unzipped over
an older version, the launcher offers "Rebuild game code".

### Highlights
- **Stage music plays.** In 0.2.0 the music of most stages (104 Opus tracks) turned into loud noise once its decoder
  opened. The decoder command the game uses (`DecodeInterleaved`) is implemented, and so are the reverb and delay
  effects the game puts on its sound
- **No black or half-drawn screen on a first visit.** While the shaders for an area build, the game pauses on its last
  complete picture with "Preparing graphics for this area... (first visit only)" and a progress bar. It also holds back
  any frame with a draw still waiting, so nothing pops in afterwards. Slow PCs had this worst: a player reported it on
  an i3-6006U with an MX110. In the stage tour, the draws shown late fell from 351 to 0 in the Arena and from 81 to 0
  in a Treasure Road stage. The cost is more short pauses on a first visit; later visits use the shader cache
- **A faster first build.** About 7 minutes on a 16-thread PC, where 0.2.0 took about 16. The slowest file starts
  first, and the few functions over 200 KB of C are compiled without optimization
- **No 6 GB image of the game's data.** The setup links your `romfs` folder into the install folder, or copies it
  when it is on another drive, instead of writing a `romfs.bin` image. On an install updated from 0.2.0, "Rebuild game code"
  sets it up this way from the folder picked before and deletes the old image
- **Mouthful Mode looks right.** The Ring, Elevator and Dump Car models were drawn without their geometry stage, so
  Ring Mouth had no face. Geometry shaders work now, and all 14,232 shader programs in the game build on the test PC

### Added
- The release `README.txt` lists the controls and how to change them
- The launcher offers "Rebuild game code" when the game code in its folder was built by another version
- The launcher refuses a build up front when the disk has less than 6.5 GB free or the install path is over 200
  characters, and refuses an unsupported game version when you pick the folder
- Clearer launcher messages for a dump of the update ("base game 1.0.0 needed"), build tools an antivirus removed, a
  game that closes at once, and a graphics driver without Vulkan
- Crash dialogs name the cause in plain words. The log repeats the fatal line after the trace, names the GPU (vendor,
  model, driver, VRAM and optional features), and after a lost GPU names the memory it was reading
- CI on GitHub (Windows): the runtime build, the engine tests, the launcher tests and repo checks, including a guard
  that fails on any game file in the repo. CI never sees game data
- `make verify GAME=<dump>` for a developer PC with the game: the generator gate, a service check, an asset check, a
  shader sweep and a stage tour (`LARECOMP_STAGE` starts at any of the 109 stages). [docs/testing.md](docs/testing.md)
  describes them
- Scripted play: `tools/input/stage1.txt` (the first stage, past the Spit guide) and `tools/input/carmouth.txt`
  (the Mouthful Mode car's boost guide). `tools/audio_check.py` checks a capture of the game's sound
- `tools/check_game_identity.py` fails when the launcher, `recomplauncher.json` and the READMEs disagree with
  `game/game.h`. [docs/launcher-runtime-contract.md](docs/launcher-runtime-contract.md) says what both launchers expect
  of the runtime

### Changed
- Built on LAS Recomp's engine (MIT). Graphics, audio, input and the in-game menu (Esc or F1) are LAS Recomp's, with
  its settings, its Controls page and its pause handling. Link's Awakening's features are left out. Windows only
- The keyboard keys changed: W A S D move, K J L I are A B X Y, Q and E are L and R, 1 and 3 are ZL and ZR, Enter is
  Plus, Backspace is Minus, the arrow keys are the d-pad, and F11 is full screen. The release `README.txt` lists
  them all
- The game code needs a CPU with AVX2 (Intel Haswell, 2013, or newer; AMD Excavator or Zen, or newer). 0.2.0 also
  built it for older CPUs
- Saves are written in one step, so a crash in the middle of a save does not corrupt it (from LAS Recomp)
- When the controller in use is unplugged, a second one takes over (from LAS Recomp)
- At boot the system calls return what the game expects (applet, BCAT, error context, parental control). A service the
  game is not allowed to use is refused as on a console, from the game's own access list
- The game code library is 119 MB (0.2.0: 109 MB), because the largest functions are compiled without optimization.
  They are 0.59% of the calls, and no runtime cost showed within the ±15% noise of the measurement

### Fixed
- The credits stopped with "out of command memory"
- Integer textures are sampled as integers, as the Vulkan spec asks (NVIDIA accepted the old way, other drivers may
  not), and optional GPU features are checked before use. A GPU without one of them failed to start. Vulkan validation
  errors from boot to the first stage went from 20,866 to 0. Not yet checked on AMD hardware
  ([#11](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/11))
- The launcher and RecompLauncher wrote the Resolution setting as `render_height`, which the new engine does not read.
  Both write `resolution` now
- Closing the launcher ends its build and every tool the build started
- Simplified and Traditional Chinese use the language codes `zh-Hans` and `zh-Hant`. Lhw-Arthas fixed this for 0.2.0
  in [#7](https://github.com/scarxity/kirby-forgotten-land-recompiled/pull/7); LAS Recomp sends the same codes. Thanks!

### Known issues
- Nobody has played the game start to finish. Boot, the opening, the first stage (scripted), Mouthful Mode's car and
  ring, and loading each stage directly in the stage tour are checked. Bosses, later worlds by real play, and the
  ending are not
- Waddle Dee Town is checked by play: a scripted run cleared the first stage and walked into the Town with no error.
  Starting the world map or the Town directly with the stage tour's shortcut crashes; that shortcut skips the story
- Stage tour, on a build from just before the release: all 109 stages load and 101 run without error. The 3 Arena
  menus, the two world maps and the Town crash only when jumped to directly; the 2 credits sequences hit the "out of
  command memory" error fixed in this version
- A first visit to an area pauses while its shaders build: about 15 to 25 s at the beach on a 2-core PC with a cold
  cache (an older measurement), less on faster PCs. Later visits pause about 1 s
- Rarely, the game crashed after the opening movie while the PC was heavily overloaded. It did not happen in 15 runs
  without other load, including runs that starved the game's threads on purpose
- A lost GPU device in Mouthful Mode ([#10](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/10),
  [#6](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/6), reported on 0.2.0) did not happen again
  in 20-minute runs at 4K. If it does, the log now names the memory the GPU was reading
- The fixes for the black screen on an AMD RX 550
  ([#11](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/11)) follow the Vulkan spec but have not
  run on AMD hardware
- Online features do not work. Choose "Do Not Use" at the prompt. amiibo and screenshots are stubs
- 3D runs at 30 fps, the game's own pace on the Switch
- Only extracted `exefs` and `romfs` folders of the base game 1.0.0 are accepted. No `.nsp` or `.xci` files,
  no update
- Frame generation is off and untested. Each tutorial clip's window may stay black for its first few seconds (seen in
  0.2.0, not checked again). No rumble. Keys are changed in the in-game menu or in `larecomp.ini` only, not in the
  launcher
- Windows only. LAS Recomp's code for other platforms is in the tree but not built

## 0.2.0

**0.1.1 could not be set up from a normal dump. This version fixes that.** The game is still not playable: it has
only been run through the opening and the start of the first stage.

No game files and no game code are included. The zip holds tools only; the game code is built on your PC from your
own extracted copy (base game, v1.0.0).

### Fixed
- "No game files were found in that folder" for a folder that holds `exefs` and `romfs`
  ([#4](https://github.com/scarxity/kirby-forgotten-land-recompiled/issues/4)). The setup looked for a folder that
  this game's data does not have, so only a folder with a ready-made `romfs.bin` was accepted
- The launcher refused folders the setup accepts: `exefs` and `romfs` under other names, or one level further down
- A folder with more than 64 folders in it (a Downloads folder) could hide the game from the search

### Changed
- The launcher and the setup now use one search, so they always agree on a folder
- The "no game files" message says what a valid folder holds: `exefs` with `main` and `main.npdm`, `romfs` with the
  folders `msg` and `gfx`

### Added
- `--check <folder>` in the runtime: says whether a folder holds the game, without changing anything

### Known issues
- Each tutorial clip's window stays black for its first few seconds
- Frame generation does not work: leave it off
- Online features do not work: choose "Do Not Use" at the prompt
- Performance: about 30 fps in 3D, with pauses while graphics pipelines compile
- Only extracted `exefs` and `romfs` folders are accepted, not `.nsp` or `.xci` files
- After unzipping a new version over an old one, build the game code again from the Set up page

## 0.1.1

The first version with a download. **The game is still not playable**: it has only been run through the opening and
the start of the first stage.

No game files and no game code are included. The zip holds tools only; the game code is built on your PC from your
own extracted copy (base game, v1.0.0).

### Highlights
- **A download that needs no developer tools.** Unzip `ForgottenLandRecomp-0.1.1-Windows-x64.zip`, open
  `ForgottenLandLauncher.exe`, pick your game folder, Build, Play. The first build takes about 16 minutes on a
  16-thread PC and about 6.5 GB of disk.
- **RecompLauncher support.** The zip carries a `recomplauncher.json`: RecompLauncher can set the game up from your
  folder, show the Window, Resolution and Volume settings, and start it.

### Added
- The release zip: the launcher, the runtime, the code translator and a compiler, with their licences
- `--setup <folder>` and `--prepare <folder>` in the runtime: the setup with no window, for the two launchers
- A notice on the launcher's Home page when the game stops unexpectedly, with the reason and a kept copy of the log
- The build progress names the file being compiled once it has taken more than a minute

### Fixed
- A crash in the first stage when a tutorial clip played: a finished movie's buffers were kept for the next movie
  although the game had already reused that memory

### Known issues
- Each tutorial clip's window stays black for its first few seconds
- Frame generation does not work: leave it off
- Online features do not work: choose "Do Not Use" at the prompt
- Performance: about 30 fps in 3D, with pauses while graphics pipelines compile
- Only extracted `exefs` and `romfs` folders are accepted, not `.nsp` or `.xci` files
- After unzipping a new version over an old one, build the game code again from the Set up page

## 0.1.0

First tagged version, source only. The game boots, plays its opening and reaches the first stage with correct
graphics.

### Added
- The generator and runtime, imported from letsgo-recomp (MIT, built on LAS Recomp) and retargeted to this game
- Shader translator support for `VOTE.VTG`, `EXIT` flow tests, `LEA`, shared-memory loads and stores, and image
  stores with Vulkan storage images
- A launcher: pick your game folder, build the game code, play
