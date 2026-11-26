# Testing

Two kinds of checks, split by one rule: **CI never sees the game.** The repo holds none, and the project never
distributes it. So CI runs only what needs no game files and no GPU. What needs the player's own extracted files runs
on a PC with one command.

## What CI checks

`.github/workflows/ci.yml` runs on every pull request and every push to `main`, on GitHub-hosted Windows. The checks
live in `tools/ci/run.sh`, so a PC runs the same thing with `make ci-test`.

| Job | Checks |
|---|---|
| `runtime` | The runtime builds (`build/ForgottenLandRecomp.exe`, MSYS2 UCRT64). LAS Recomp's game-free tests: build cache, process handling, delete guard, data move, movie extractor, kernel condition variable, rumble, active pad, Vulkan fence, validation, overlay and sync-error regressions (simulated, no driver), guest heap, thread stack, lazy commit, SIMD rounding, settings, pacing, LED, crash dialog. This repo's tests of the command memory spill and the audio effects. The shader translator's fuzz run and its output through `glslc` and `spirv-val`. The find-game test on synthetic dumps. |
| `linux` | The Linux build ([linux.md](linux.md)), in the container of `tools/linux/Containerfile` on an Ubuntu runner: the dependencies (SDL3, the LGPL FFmpeg with its decoders checked, shaderc), the runtime, the game-free tests that run on Linux (`tools/ci/run.sh build engine`; the Windows-only ones and the ones that need `glslc` or `spirv-val` are SKIP), and the AppImage, kept as a workflow artifact for seven days. Nothing in it plays the game. |
| `launcher` | `cargo test --locked` in `launcher/src-tauri`, `node --test` in `launcher`. |
| `repo` | No game data in the tree (`tools/ci/no_game_data.sh`: Switch game extensions, `romfs/` `exefs/` `generated/`, `lasgame.*`, keys, any file over 1 MiB outside `third_party/`). The upstream sync tool's test. The generator's `--dyn-base` test. The issue bot's and the shader collector's tests on synthetic log text (`tools/test_log_triage.py`, `tools/test_shader_collect.py`, see [issue-triage.md](issue-triage.md)). The self-test of every `tools/verify/*.py` that has a `--self-test` flag, and of `tools/check_game_identity.py` once it has one. |

Checks that cannot run yet show as `PENDING`, ones that cannot run on Windows as `SKIP`, and ones known to fail in the
tree as `XFAIL`, each with its reason, in the summary at the end of the log. The reasons are in `tools/ci/run.sh`.

Run it:

```sh
make ci-test            # builds, then runs the engine and repo suites
bash tools/ci/run.sh engine repo     # the suites alone (build, engine, repo); needs the runtime built
```

In Windows, run these in an MSYS2 UCRT64 shell (the README's "Build from source"), with Python packages
`capstone==5.0.9 unicorn==2.1.4 lz4==4.4.5` for the `repo` suite. `PY=<python>` picks the interpreter.

## What `make verify` checks

```sh
make verify GAME=<folder holding exefs/ and romfs/>
```

Runs `tools/ci/verify.sh`. Output goes to `<folder>/../verify-out`, outside the repo, because it describes the game.
A check whose tool is not in `tools/verify/` yet prints `not yet`; the run goes on, and the exit code is 1 until every
check has run and passed.

| Check | Proves | Tool |
|---|---|---|
| Generator gate | Every instruction lifts, every branch target resolves | `tools/verify/gen_gate.py` |
| Service check | Every service the game names is implemented | `tools/verify/service_check.py` |
| Asset decode | Every RomFS file reads, every movie and sound decodes | `tools/verify/asset_check.py` |
| Shader sweep | Every shader translates, validates and builds a pipeline on this PC's driver | `tools/verify/shader_sweep.sh` |
| Stage tour | Every stage loads and renders without errors | `tools/verify/stage_tour.sh` |

## Stutter on a first visit

The stage tour passes with any amount of stutter, so a change that touches shader builds or frame holds is also
compared on a cold cache, the way a player's first run sees it:

```sh
rm -rf <work>/cache    # the shader caches
KEY=$((RANDOM * 32768 + RANDOM + 2))
# the launcher's prepare step: the game started hidden, every shader of the game files prepared, then it exits
LARECOMP_VK_COLD=$KEY LARECOMP_HIDDEN=1 LARECOMP_NO_USER_INPUT=1 LARECOMP_SHADER_PREP=1 LARECOMP_SHADER_PREBUILD=1 \
    LARECOMP_SHADER_PREP_EXIT=1 <work>/build/ForgottenLandRecomp.exe extracted --play
LARECOMP_VK_COLD=$KEY TOUR_ONLY='^(Level1/Stage2|CstageFire/Stage01|Level2/Stage1)/' WORK=<work> \
    bash tools/verify/stage_tour.sh <game folder> <out>
python tools/verify/hold_stats.py <out>
```

`LARECOMP_VK_COLD=<n>` makes every pipeline new to the driver, as on a player's first run; the prepare process and the
play process must have the same number. Test the two processes, as the launcher runs them: a single process, or a
prepare step that is killed, hides misses. `hold_stats.py` prints the frames held (what a player feels as stutter),
those of 50 ms or more and the area-entry bursts. Each `[vk] frame held` log line says what the wait was for (shaders
translated, compiled, pipelines built).
0.3.2 measured on an RTX 3050 Laptop, first stage: holds of 50 ms or more 52 -> 9, the longest 1.4 s -> 0.13 s.

## Shaders prepared before play, from any launcher

`--play` prepares the shaders itself when the stamp (`cache/shaders-prepared.txt`, [launcher-runtime-contract.md](launcher-runtime-contract.md))
does not match this runtime, game code, shipped list, mods and GPU driver. The rule is `tools/test_shader_stamp.c`
(`make ci-test`, no game). End to end, on a copy of the work folder, hidden and muted, the way a player's start does it
(no `LARECOMP_NO_LAUNCHER`, which marks a test run and skips the check):

```sh
export LARECOMP_NO_AUDIO=1 LARECOMP_HIDDEN=1
<work>/build/ForgottenLandRecomp.exe --shaders-status           # prepare none
<work>/build/ForgottenLandRecomp.exe extracted --play           # log: "[shader] not prepared ... (none)", "[shader] preparing: N%", stamp written, then the game boots
<work>/build/ForgottenLandRecomp.exe --shaders-status           # ready
<work>/build/ForgottenLandRecomp.exe extracted --play           # no "preparing" lines: boots at once
sed -i 's/^gpu .*/gpu another driver/' <work>/cache/shaders-prepared.txt   # a driver change, as the stamp sees it
<work>/build/ForgottenLandRecomp.exe --shaders-status           # prepare gpu
```

## What no check proves

That the game can be finished. The checks show that every stage's code, data and shaders work and that no known
failure class is left. Bosses, unlocks and the ending need people to play them: release a testing build first.

## Not done: a self-hosted runner

A runner on the PC that holds the game could run the `make verify` checks on every pull request. This repo is public,
and a pull request from a fork runs its own workflow code on that runner, so on that PC and next to the game. It
stays off. If it is ever wanted, run it only for pushes to this repo's own branches, with approval required for all
outside collaborators (Settings, Actions), and not on the PC the player uses.
