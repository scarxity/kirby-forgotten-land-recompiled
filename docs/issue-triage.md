# Issue triage and the shipped shader list

`runtime/nvn/shader_variants.txt` grows from what players' games meet. A game writes a "[shader] record ..." line to
`game.log` for each shader variant the shipped list lacks (`runtime/nvn/shader_seen.c`), and keeps them all in
`cache/shader_seen.txt`. Both hold hashes and draw settings only. Players send them with their bug reports, and two
workflows turn the reports into a pull request. Nothing is merged without a person reading it.

## How records arrive

1. A player reports a problem with the launcher's **Report a problem** button (About page), which opens the issue form
   (`.github/ISSUE_TEMPLATE/bug-or-performance.yml`) and the folder with `game.log`. The form asks for the version, the
   launcher used, the GPU, and the files: `game.log` (or `crash-*.log`), and `cache\shader_seen.txt` for stutter.
   RecompLauncher players find `kirby-forgotten-land-recompiled.log` in `%APPDATA%\com.recomplauncher.app\game-output\`.
2. **Log triage** (`.github/workflows/log_triage.yml`, `tools/log_triage.py`) runs on every new or edited issue and
   comment. It reads the attached `game.log`, `game-backup-*.log`, `crash-*.log`, RecompLauncher's log and
   `shader_seen.txt`, and keeps one comment on the issue: build, system, GPU, settings, how the run ended, frame rate,
   notable lines. The issue gets `log: crash`, `log: device-lost` or `log: ok`. Records the shipped list lacks go in a
   folded part of the comment, and the issue gets **`log: new shaders`**.
3. **Shader list collection** (`.github/workflows/shader_collect.yml`, `tools/shader_collect.py`) runs every Monday and on
   demand. It reads every issue with `log: new shaders`, merges their records into the list, runs
   `tools/ci/no_game_data.sh`, and opens or updates one pull request from the branch `shader-list/collect` with the
   counts and the issue numbers. The same issues always give the same file. A pull request opened with the workflow
   token starts no CI run: close and reopen it before merging.

Read the pull request, then merge it. A record that matches no shader of the game is harmless (the prepare step skips
it), but the list is shipped to everyone: look at the counts before merging.

## Merging by hand

```bash
python tools/shader_list_merge.py runtime/nvn/shader_variants.txt game.log shader_seen.txt   # logs or shader_seen.txt files
bash tools/ci/no_game_data.sh
```

The merge lets only lines of the record shape in (the shape `no_game_data.sh` checks) and says how many other lines it
skipped. Records are only ever added, so a re-run changes only what is new.

## What the workflows never do

- Put text of an issue, a comment or a log into a workflow expression, a shell, or the pull request's text. The scripts
  get an issue number and read the rest through the API.
- Download from anywhere but GitHub's attachment host, or more than 20 MB per file. A log is only parsed.
- Quote a shader record that is not exactly the shape of a shipped one.
- Check out or run code from a pull request. Issue events run the default branch's script.

## Testing

`python tools/test_log_triage.py` and `python tools/test_shader_collect.py` run on synthetic text (`bash tools/ci/run.sh
repo` runs them). Issue events run from the default branch only, so the workflows themselves are tried from the Actions
tab after they are merged: **Log triage** (an issue number, dry run on by default) prints the comment in the job's summary
and posts nothing, **Shader list collection** (dry run on by default) prints the pull request's text. Locally,
`python tools/log_triage.py parse game.log shader_seen.txt` prints the comment for local files.

Setup once: Settings > Actions > General > "Allow GitHub Actions to create and approve pull requests" for the collection.
