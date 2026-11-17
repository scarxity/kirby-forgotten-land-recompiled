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
| `launcher` | `cargo test --locked` in `launcher/src-tauri`, `node --test` in `launcher`. |
| `repo` | No game data in the tree (`tools/ci/no_game_data.sh`: Switch game extensions, `romfs/` `exefs/` `generated/`, `lasgame.*`, keys, any file over 1 MiB outside `third_party/`). The upstream sync tool's test. The generator's `--dyn-base` test. The self-test of every `tools/verify/*.py` that has a `--self-test` flag, and of `tools/check_game_identity.py` once it has one. |

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
rm -rf <work>/cache    # the pipeline cache
LARECOMP_VK_COLD=1 TOUR_ONLY='^(Level1/Stage2|CstageFire/Stage01|Level2/Stage1)/' WORK=<work> \
    bash tools/verify/stage_tour.sh <game folder> <out>
python tools/verify/hold_stats.py <out>
```

`LARECOMP_VK_COLD=1` makes every pipeline new to the driver's own cache too. Run each side at least three times,
alternating. `hold_stats.py` prints the frames held outside area entry (what a player feels as stutter), those of 50 ms
or more, the area-entry bursts (a new area's shaders), and the frames shown with a few draws late.
0.3.2 measured on an RTX 3050 Laptop: holds of 50 ms or more 35-37 -> 12 a run, about 60 frames shown with draws late.

## What no check proves

That the game can be finished. The checks show that every stage's code, data and shaders work and that no known
failure class is left. Bosses, unlocks and the ending need people to play them: release a testing build first.

## Not done: a self-hosted runner

A runner on the PC that holds the game could run the `make verify` checks on every pull request. This repo is public,
and a pull request from a fork runs its own workflow code on that runner, so on that PC and next to the game. It
stays off. If it is ever wanted, run it only for pushes to this repo's own branches, with approval required for all
outside collaborators (Settings, Actions), and not on the PC the player uses.
