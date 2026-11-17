# Status

This page says how far the game gets in 0.3.0, how each part was checked, and what is known to be wrong.
[testing.md](testing.md) describes the checks. [decisions.tsv](decisions.tsv) records the decisions of 0.1.0 to
0.2.0 and their evidence.

Supported dump: the base game, v1.0.0 (program id `01004D300C5AE000`, `main` build id `d9ba7db72ffafecd`).

All numbers are from the developer PC (RTX 3050 Laptop 4 GB, 16 threads) unless a line says otherwise.

## What works

| Part | State | How it was checked |
|---|---|---|
| Game code | `main`, `sdk` and `subsdk0` lift to C and build into `lasgame.dll` | Generator gate (`make verify`): 0 unsupported instructions in all 3 modules |
| Boot | SDK and game start-up, services, RomFS, threads, Vulkan device and window | Service check (`make verify`). Vulkan validation from boot to the first stage: 0 errors |
| Game data | Every RomFS file reads | Asset check (`make verify`): 9,373 files, all 36 movies and all 104 Opus files decode, all 25,689 textures are in supported formats |
| Shaders | Every shader program translates and builds | Shader sweep (`make verify`): 14,232 programs, 0 failures |
| Opening | The online prompt, the difficulty pick, the opening movie and cutscene | Scripted run: the capture at frame 3400 (the beach) matches v0.2.0 |
| First stage | Kirby walks, the scene is lit, the guide clips play | Scripted runs: the capture at frame 5000 matches v0.2.0; `tools/input/stage1.txt` goes past the Spit guide |
| Mouthful Mode | The car and the ring, drawn with their geometry stage | `tools/input/carmouth.txt` (the car's boost guide) |
| Into the Town | The first stage played to its end, the rescue cutscene, then Waddle Dee Town | A scripted run on 0.3.0 cleared the first stage and stayed about 4 minutes in the Town (frames 18,840 to 25,920) with no error |
| Every stage | `LARECOMP_STAGE` starts a new game at any of the 109 stages | Stage tour (`tools/verify/stage_tour.sh`), one run per stage, on a build from just before the release (before the last frame-hold fix): all 109 stages load and 101 run without error. The other 8: the 3 Arena menus, the two world maps and the Town crash when jumped to directly (the Town runs when reached by play), and the 2 credits sequences stopped with "out of command memory", fixed since (the story credits checked again on 0.3.0: no error, no late draws) |
| Music and sound | Stage music plays through the game's Opus decoder; the reverb and delay effects run | In a v0.2.0 first-stage run the game called the decoder command `DecodeInterleaved` 6,922 to 14,254 times and got no samples back; it decodes now. `tools/test_audio_fx.c` checks the effects with the game's own parameters |
| Launcher | Pick the game folder, build, cancel, rebuild, play | Unit tests in CI (`cargo test`, `node --test`). From a fresh unzip of the 0.3.0 zip: Build (about 6 minutes, PC shared) and Play reach the first stage; so does RecompLauncher's `--prepare` and start command; so does "Rebuild game code" over a 0.2.0 install |

Reproduce the opening and the first stage with `bash tools/run_scripted.sh <name> 170 "3400,5000"`, then look at
`<work>/runs/<name>/fin_3400.png` and `fin_5000.png`. `tools/boot_check.sh` runs a timed boot without input.
`make verify GAME=<dump>` runs every check above that needs the game.

## Known problems

- Nobody has played the game start to finish. Bosses, the later worlds by real play, and the ending are not checked.
- Starting the world map or Waddle Dee Town directly with `LARECOMP_STAGE` (a test shortcut that skips the story)
  crashes. Reached by play, the Town runs without error.
- A first visit to an area pauses on its last complete picture while its shaders build. On a
  2-core PC with a cold cache the pause at the beach was about 15 to 25 s (an older measurement). Fast PCs pause less.
  Later visits use the shader cache: the longest pause on a second run was about 1.2 s, with a cold driver cache.
- In an area already entered, a new effect or enemy can still pause the picture for up to 33 ms while its shaders
  build, or appear a frame late (0.3.2; 12 pauses of 50 ms or more a run on the three-stage cold test, down from 36).
- Rarely, the game crashed after the opening movie while the PC was heavily overloaded: a UI script read an empty
  widget. It did not happen in 15 runs without other load, including runs that starved the game's threads on purpose
  and 3,265 injected pauses.
- A lost GPU device in Mouthful Mode was reported on 0.2.0 (issues #10 and #6). It did not happen on 0.2.0 or 0.3.0 in
  20-minute runs at 4K on the developer PC. The log now names the memory the GPU was reading.
- On AMD, an RX 550 showed a black screen (issue #11). The fixes follow the Vulkan spec, and a software Vulkan driver
  (lavapipe) boots and draws the beach, but nothing has run on AMD hardware.
- Online features do not work. Choose "Do Not Use" at the prompt.
- 3D runs at 30 fps, the game's own pace on the Switch.
- Only extracted `exefs` and `romfs` folders of the base game 1.0.0 are accepted. No `.nsp` or `.xci` files, and no
  update.
- Frame generation is off and untested.
- Each tutorial clip's window may stay black for its first few seconds. This was seen in 0.2.0 and not checked again.
- No rumble.
- Keys are changed in the in-game menu (Controls) or in `larecomp.ini`, not in the launcher.
- Windows only. LAS Recomp's code for Linux, macOS, iOS and Android is in the tree but not built.

## Stubs and shortcuts

- `nfp:user` (amiibo) behaves like a console that never sees an amiibo.
- Screenshots (`caps:su`, `caps:u`), NFC, `friend:u`, `mii:u` and the other services the game may use but has not
  called in testing answer through the logging stub, which returns success.
- `prepo:u` keeps play reports and never sends them. `bcat:u` has no deliveries. `nifm`, `ssl`, `bsd` and `ldn` act
  as an offline console.
- Shaders are not built ahead of time. LAS Recomp's shader preparation has no variant list for this game
  (`runtime/nvn/shader_variants.txt` is empty), so each pipeline builds the first time it is drawn.
- A compute shader that indexes shared memory with a register gets a fixed 32 KB of it.
- Image stores (`SUST`) work only in compute shaders and only with all four components.
- The translator still rejects some instruction forms, such as `LEA.X`. The shader sweep shows the game uses none of
  them.
