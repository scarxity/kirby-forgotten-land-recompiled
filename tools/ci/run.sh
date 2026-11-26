#!/bin/bash
# The checks CI runs, in one place so a PC runs exactly what the workflow runs (make ci-test).
#   tools/ci/run.sh build            the runtime (build/<EXE_NAME>.exe)
#   tools/ci/run.sh engine           LAS Recomp's game-free tests, the shader translator's SPIR-V checks, the find-game test
#   tools/ci/run.sh repo             the no-game-data guard, the upstream sync test, the generator test, the issue bot's and
#                                    shader collector's tests, tools/verify self-tests
# Any number of suites in one call. Every check runs even after one fails; the exit code is 1 if any failed. A check
# that cannot run yet is listed as PENDING, one that cannot run here as SKIP. Needs no game files and no GPU.
# Env: PY (python, default "python"), EXE (the runtime, default build/<EXE_NAME>.exe from game/game.mk), MAKEARGS (more
# arguments for the runtime's make). On Linux (the container of tools/linux/Containerfile; the workflow's linux job) the
# tests of the Windows code and the ones that need glslc or spirv-val are listed as SKIP; EXE and MAKEARGS name the build
# (build/linux/x86_64/game/ForgottenLandRecomp, B= DEPS= SHADERC=...), CPATH, LIBRARY_PATH, LD_LIBRARY_PATH and
# PKG_CONFIG_PATH the dependencies' folders.
cd "$(dirname "$0")/../.." || exit 2
PY=${PY:-python}
[ "$(uname -s)" = Linux ] && LINUX=1 || LINUX=
EXE=${EXE:-build/$(sed -n 's/^EXE_NAME *:= *//p' game/game.mk).exe}
mkdir -p build
passed=0 failed=0 other=0 summary=""

t() {  # t <name> <command>: the command runs in a subshell with -e
    local name=$1 start=$SECONDS
    echo "::group::$name"
    if ( set -e; eval "$2" ); then
        echo "::endgroup::"; passed=$((passed + 1)); summary+="PASS    $name ($((SECONDS - start))s)"$'\n'
    else
        echo "::endgroup::"; echo "::error title=$name::failed"
        failed=$((failed + 1)); summary+="FAIL    $name ($((SECONDS - start))s)"$'\n'
    fi
}
xfail() {  # xfail <name> <why> <command>: fails in the tree today; reported, not fatal. XPASS means it is fixed: use t
    local name=$1 why=$2 start=$SECONDS
    echo "::group::$name (known failing)"
    if ( set -e; eval "$3" ); then
        echo "::endgroup::"; echo "::warning title=$name::passes now, run it with t"; other=$((other + 1)); summary+="XPASS   $name: $why"$'
'
    else
        echo "::endgroup::"; echo "::warning title=$name::known failing: $why"; other=$((other + 1)); summary+="XFAIL   $name: $why"$'
'
    fi
}
note() {  # note <PENDING|SKIP> <name> <why>
    echo "$1 $2: $3"; other=$((other + 1)); summary+="$1 $2: $3"$'\n'
}

suite_build() {
    t "runtime build" 'make -j"$(nproc)" $MAKEARGS "$EXE"'
}

suite_engine() {
    t build_cache '$PY tools/test_build_cache.py'
    # (miniz first: the test would fetch it with the first bash on PATH, WSL's on a hosted runner)
    t gamebuild_resume 'bash tools/fetch_miniz.sh && $PY tools/test_gamebuild_resume.py'
    if [ "$LINUX" ]; then note SKIP windows_processes "the Windows process code"; else
    t windows_processes '$PY tools/test_windows_processes.py'; fi
    t delete_guard '$PY tools/test_delete_guard.py'
    t data_move '$PY tools/test_data_move.py'
    if [ "$LINUX" ]; then note SKIP movie_extractor "reads a WAV file, which the game's minimal FFmpeg (tools/linux/build_deps.sh) has no demuxer for; MSYS2's full one does"; else
    t movie_extractor 'clang -std=gnu11 -O1 tools/test_movie_extractor.c -lavformat -lavcodec -lavutil -lswscale -o build/test-movie-extractor.exe && rm -rf build/movie-test && mkdir -p build/movie-test && build/test-movie-extractor.exe build/movie-test'; fi
    t condvar 'clang -O2 -pthread tools/test_condvar.c -o build/test-condvar.exe && build/test-condvar.exe'
    t cmdbuf 'clang -O1 tools/test_cmdbuf.c -o build/test-cmdbuf.exe && build/test-cmdbuf.exe'
    t tegra 'clang -std=gnu11 -O2 tools/test_tegra.c -o build/test-tegra.exe && build/test-tegra.exe'
    t rumble 'clang -std=gnu11 -O2 $(pkg-config --cflags sdl3) tools/test_rumble.c $(pkg-config --libs sdl3) -o build/test-rumble.exe && build/test-rumble.exe'
    t active_pad 'clang -std=gnu11 -O2 $(pkg-config --cflags sdl3) tools/test_active_pad.c $(pkg-config --libs sdl3) -o build/test-active-pad.exe && build/test-active-pad.exe'
    t fence_sequence '$PY tools/test_fence_sequence.py'
    t gmem_lowmem '$PY tools/test_gmem_lowmem.py'
    t vulkan_validation '$PY tools/test_vulkan_validation.py'
    t vulkan_overlay '$PY tools/test_vulkan_overlay.py'
    t vulkan_sync_errors '$PY tools/test_vulkan_sync_errors.py'
    if [ "$LINUX" ]; then note SKIP thread_stack "the Windows thread stack code"; else
    t thread_stack 'clang -std=gnu11 -O1 tools/test_windows_thread_stack.c -lpsapi -o build/test-thread-stack.exe && build/test-thread-stack.exe'; fi
    if [ "$LINUX" ]; then note SKIP lazy_commit "the Windows lazy commit code"; else
    t lazy_commit 'clang -std=gnu11 -O1 -D_FILE_OFFSET_BITS=64 -D_POSIX_THREAD_SAFE_FUNCTIONS -DWIN32_LEAN_AND_MEAN -DNOMINMAX tools/test_windows_lazy_commit.c -lwinmm -lole32 -lshell32 -o build/test-lazy-commit.exe && build/test-lazy-commit.exe'; fi
    if [ "$LINUX" ]; then note SKIP windows_logs "the Windows session log"; else
    t windows_logs '$PY tools/test_windows_logs.py'; fi
    t simd_rounding 'clang -std=gnu11 -O1 -mfma -pthread tools/test_simd_rounding.c -lm -o build/test-simd-rounding.exe && build/test-simd-rounding.exe'
    t settings '$PY tools/test_settings.py'
    t pace '$PY tools/test_pace.py'
    t led 'clang -std=gnu11 -O1 tools/test_led.c -lm -o build/test-led.exe && build/test-led.exe'
    t audio_fx 'clang -O2 tools/test_audio_fx.c -lm -o build/test-audio-fx.exe && build/test-audio-fx.exe'
    t crash_explain 'clang -std=gnu11 -O1 tools/test_crash_explain.c -o build/test-crash-explain.exe && build/test-crash-explain.exe'
    t mods 'clang -std=gnu11 -O1 tools/test_mods.c runtime/mods.c -o build/test-mods.exe && build/test-mods.exe'
    t shader_stamp 'clang -std=gnu11 -O1 tools/test_shader_stamp.c runtime/shader_stamp.c -o build/test-shader-stamp.exe && build/test-shader-stamp.exe'
    # (GLSL only: the script's MSL comparison needs a reference commit; REF=HEAD satisfies it, the GLSL half is the check)
    if [ "$LINUX" ]; then note SKIP translate_glsl "needs SPIRV-Tools' library and glslc, which the Linux image lacks (the Windows job runs it)"; else
    t translate_glsl 'REF=HEAD tools/test_translate.sh 4000 "$(nproc)" glsl'; fi
    t translate_ops 'tools/test_translate_ops.sh'
    if [ "$LINUX" ]; then note SKIP spirv_val "needs glslc and spirv-val, which the Linux image lacks (the Windows job runs it)"; else
    t spirv_val 'tools/test_spirv_val.sh'; fi
    note SKIP translate_flow "uses setenv, which the Windows toolchain lacks (LAS runs it on Linux and macOS)"
    note SKIP dynres "uses setenv (LAS runs it on Linux and macOS)"
    note SKIP update_tags "checks LAS Recomp's release tags and build numbers, not ours"
    note SKIP game_addresses "reads a version table (LAS has three game versions); ours has one"
    note SKIP menu_strings "checks LAS Recomp's menu texts against its translations"
    if grep -rqs -e '"--check"' runtime; then
        t find_game 'bash tools/test_find_game.sh "$EXE"'
    else
        note PENDING find_game "the runtime has no --check yet (tools/test_find_game.sh runs once it does)"
    fi
}

suite_repo() {
    t no_game_data 'sh tools/ci/no_game_data.sh'
    t upstream_sync 'bash tools/upstream/test_sync.sh'
    t dyn_base '$PY tools/test_dyn_base.py'
    t log_triage '$PY tools/test_log_triage.py'
    t shader_collect '$PY tools/test_shader_collect.py'
    local f flag
    for f in tools/verify/*.py tools/check_game_identity.py; do
        [ -f "$f" ] || continue
        case $f in tools/verify/test_*.py) flag= ;; *) flag=$(grep -o -m1 -e '--self-test' -e '--selftest' "$f") ;; esac
        if [ -n "$flag" ] || [ "${f#tools/verify/test_}" != "$f" ]; then t "selftest $f" "\$PY $f $flag"
        else note SKIP "$f" "no --self-test"; fi
    done
}

[ $# -gt 0 ] || { sed -n '2,6p' "$0"; exit 2; }
for s in "$@"; do
    case $s in build | engine | repo) "suite_$s" ;; *) echo "unknown suite $s"; exit 2 ;; esac
done
echo; echo "---- summary"; printf '%s' "$summary"
echo "passed $passed, failed $failed, pending or skipped $other"
[ "$failed" = 0 ]
