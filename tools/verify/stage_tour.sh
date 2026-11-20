#!/bin/bash
# stage_tour.sh <game folder> <out folder>: load every stage of the game, one run each, and judge each run
# (tools/verify/stage_check.py). The stages are the step folders of the player's RomFS (romfs/ubin/Step/Map/<Category>/
# <Stage>/<StepNN>/), the first step of each stage; TOUR_STEPS=all takes every step. Each run starts a new game on a
# scratch save with LARECOMP_STAGE=<step> (runtime/hle/stage_jump.c), plays the boot input to the stage, then light
# input while the stage holds, and ends with a window capture at TOUR_END. Output (logs, captures, tour.jsonl,
# report.md) goes to <out folder> only: it describes the game.
#   WORK=<folder with build/ForgottenLandRecomp.exe and build/lasgame.dll> (default ../work/kfl, as run_scripted.sh)
#   TOUR_ONLY=<regex>   only the steps matching it      TOUR_LOOP=1   start over at the end (soak), until stopped
#   TOUR_END=<frame>    last frame of a run (6600)      TOUR_HOLD=<frames> after the step loads before skipped draws
#   TOUR_SECS=<s>       wall-clock limit of a run (600)                      count (600)
#   TOUR_WRAP=<command> a prefix for each game run (a queue that holds the PC for one run at a time)
#   TOUR_SEED=<n>       random play instead of the light input (soak: with a large TOUR_END / TOUR_SECS, or TOUR_LOOP)
set -u
GAME=$(cd "${1:?usage: stage_tour.sh <game folder> <out folder>}" && pwd)
OUT=${2:?usage: stage_tour.sh <game folder> <out folder>}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(cd "${WORK:-$ROOT/../work/kfl}" && pwd)
END=${TOUR_END:-6600} HOLD=${TOUR_HOLD:-600} SECS=${TOUR_SECS:-600}
PY=${PY:-python}
mkdir -p "$OUT" && OUT=$(cd "$OUT" && pwd)
win() { cygpath -m "$1" 2>/dev/null || echo "$1"; }  # (paths handed to the game in variables: Windows form there)
[ -x "$WORK/build/ForgottenLandRecomp.exe" ] || { echo "no $WORK/build/ForgottenLandRecomp.exe (set WORK)"; exit 2; }

MAP="$1/romfs/ubin/Step/Map"
steps() {
    if [ "${TOUR_STEPS:-}" = all ]; then (cd "$MAP" && ls -d */*/Step*/ | sed 's:/$::')
    else (cd "$MAP" && for s in */*/; do ls -d "$s"Step*/ | head -1; done | sed 's:/$::'); fi | grep -E "${TOUR_ONLY:-.}"
}
STEPS=$(steps)
[ -n "$STEPS" ] || { echo "no map steps under $MAP"; exit 2; }
echo "[tour] $(echo "$STEPS" | wc -l) steps, $END frames each, output in $OUT"

# The boot (as run_scripted.sh: online prompt, difficulty, the opening movie skipped, cutscenes), then light play.
BOOT="880:LEFT:8,950:A:8,1400:A:8,1800:A:8,2600:A:8,2700:PLUS:8,2800:B:8,3400:A:8,3550:A:8,3700:A:8,3850:A:8,4000:A:8,4150:A:8,4300:A:8,4450:A:8"
PLAY="4750:STICK_UP:120,4900:A:8,5000:STICK_RIGHT:120,5150:B:30,5300:STICK_LEFT:120,5450:A:8,5550:STICK_DOWN:120,5700:B:30,5850:STICK_UP:200,6100:A:8,6200:STICK_RIGHT:200"
if [ -n "${TOUR_SEED:-}" ]; then  # random play from that seed up to TOUR_END instead (soak runs; no PLUS: the pause menu)
    # shortcut: the engine reads at most 16 KB of LARECOMP_INPUT (hle/input.c), about 15 minutes of this; longer soaks
    # loop the tour (TOUR_LOOP=1) until the input can come from a file
    PLAY=$(awk -v seed="$TOUR_SEED" -v end="$END" 'BEGIN { srand(seed); n = split("A B X Y L R ZL ZR STICK_UP STICK_DOWN STICK_LEFT STICK_RIGHT STICK_45 STICK_135 STICK_225 STICK_315", k, " ")
        for (f = 4750; f < end && length(s) < 15500; f += 30 + int(rand() * 60)) s = s sprintf("%s%d:%s:%d", s ? "," : "", f, k[1 + int(rand() * n)], 8 + int(rand() * 60))
        print s }')
fi

run_step() {  # run_step <step>: one run, its record appended to tour.jsonl
    local step=$1 id=${1//\//_}
    local log="$OUT/$id.log" shot="$OUT/$id.png" wav="$OUT/$id.wav"
    rm -rf "$OUT/save" "$wav" "$shot" && mkdir -p "$OUT/save"
    (
        cd "$WORK" || exit 2
        export PATH="/c/msys64/ucrt64/bin:$PATH"
        export LARECOMP_NO_LAUNCHER=1 LARECOMP_GAME_LIB=build/lasgame.dll LARECOMP_SAVE_DIR="$(win "$OUT/save")" LARECOMP_HIDDEN=1
        export LARECOMP_NO_USER_INPUT=1 LARECOMP_INPUT="$BOOT,$PLAY" LARECOMP_STAGE="$step" LARECOMP_LOG_FILES=1 LARECOMP_LOG_SKIPS=1
        export LARECOMP_NO_AUDIO=1 LARECOMP_AUDIO_WAV="$(win "$wav")"  # (the game's sound to a file, no audio device opened)
        export LARECOMP_WINDOW_SHOT=$END LARECOMP_SHOT_PATH="$(win "$shot")" LARECOMP_EXIT_AFTER_SHOT=1
        ${TOUR_WRAP:-} bash "$ROOT/tools/verify/stage_run.sh" "$SECS" "$log" "$GAME"
    )
    "$PY" "$ROOT/tools/verify/stage_check.py" check "$step" "$log" "$shot" "$wav" "$HOLD" >> "$OUT/tour.jsonl"
    tail -1 "$OUT/tour.jsonl" | grep -q 'silent"' || rm -f "$wav"  # (tens of MB a run: kept only when it was silent)
    tail -1 "$OUT/tour.jsonl" | "$PY" -c "import json,sys; r=json.load(sys.stdin); print('[tour] %s: %s' % (r['stage'], r['verdict']))"
}

: > "$OUT/tour.jsonl"
while :; do
    for step in $STEPS; do run_step "$step"; done
    [ "${TOUR_LOOP:-}" = 1 ] || break
    echo "[tour] loop"
done
"$PY" "$ROOT/tools/verify/stage_check.py" report "$OUT/tour.jsonl" | tee "$OUT/report.md"
