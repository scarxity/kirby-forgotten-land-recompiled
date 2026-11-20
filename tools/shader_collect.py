#!/usr/bin/env python3
"""Collect the shader records players attached to issues into one pull request.

.github/workflows/shader_collect.yml runs it weekly and on demand, in three steps:

  shader_collect.py collect   every issue labelled "log: new shaders" (and its comments) is read through the GitHub API;
                              the game logs and shader_seen.txt files attached to it are downloaded with
                              tools/log_triage.py's rules (GitHub's attachment host only, MAX_BYTES each) and merged
                              into runtime/nvn/shader_variants.txt with tools/shader_list_merge.py, which lets only
                              lines of the record shape in. Writes the summary the next step uses.
  bash tools/ci/no_game_data.sh   (the workflow) the list still holds only records
  shader_collect.py publish   when collect added records: the list is committed to the branch shader-list/collect
                              (reset to the default branch's head first, so the same issues always give the same
                              file) and the one pull request from it is opened or updated. Nothing is ever merged.

The pull request's text is made here from numbers and issue numbers: no text of an issue or a log is put in it.
Everything downloaded is data: parsed, never run or handed to a shell. A run needs only issues: read for `collect`;
`publish` needs contents: write and pull-requests: write.

env: GITHUB_TOKEN, GITHUB_REPOSITORY, DRY_RUN=1 (collect only reads: it never posts or writes to GitHub; publish prints
what it would do), COLLECT_SUMMARY (the summary's path, default $RUNNER_TEMP/shader_collect.json), BASE (the default
branch, publish only).
Standard library only.
"""
import base64
import json
import os
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)  # (python -I leaves the script's folder off the path)
import log_triage  # noqa: E402

LABEL = log_triage.SHADER_LABEL[0]
LIST = "runtime/nvn/shader_variants.txt"
BRANCH = "shader-list/collect"
MAX_FILES = 300        # downloads per run
MAX_NEW = 5000         # new records per run: more is for a person to look at
TITLE = "Shader list: records from players' logs"


def gather(api):
    """every game log / shader list attached to the labelled issues or their comments -> ([(issue number, url)] by issue
    number, the issue numbers). Pull requests carrying the label are not issues."""
    q = urllib.parse.quote(LABEL)
    issues = [i for i in api.pages(f"issues?labels={q}&state=all&per_page=100") if "pull_request" not in i]
    found, seen = [], set()
    for issue in sorted(issues, key=lambda i: i["number"]):
        n = issue["number"]
        bodies = [issue.get("body")]
        for c in api.pages(f"issues/{n}/comments?per_page=100"):
            if (c.get("user") or {}).get("type") != "Bot":
                bodies.append(c.get("body"))
        for body in bodies:
            for u in log_triage.attachments(body):
                if u not in seen:
                    seen.add(u)
                    found.append((n, u))
    return found, sorted(i["number"] for i in issues)


def collect(api, root):
    """downloads the attachments, merges them into root/LIST; returns the summary (also what publish reads)"""
    found, issues = gather(api)
    shipped = log_triage.shipped_records(os.path.join(root, LIST))
    if shipped is None:
        sys.exit(f"{LIST} not found in {root}")
    skipped = [f"{len(found) - MAX_FILES} attachments over the limit of {MAX_FILES}"] if len(found) > MAX_FILES else []
    per_issue, files, problems = {}, [], []
    with tempfile.TemporaryDirectory() as tmp:
        for k, (n, u) in enumerate(found[:MAX_FILES]):
            try:
                text = log_triage.download(u)
            except Exception as e:  # (a bad download is reported, never fatal)
                problems.append(f"#{n}: {type(e).__name__}")
                continue
            new = log_triage.parse(text, shipped)["records"]
            if new:
                per_issue.setdefault(n, set()).update(new)
                path = os.path.join(tmp, f"{k:04d}.txt")
                with open(path, "w", encoding="utf-8", newline="\n") as f:
                    f.write(text)
                files.append(path)
        before = len(shipped)
        added = sorted(set().union(*per_issue.values())) if per_issue else []
        if len(added) > MAX_NEW:
            sys.exit(f"{len(added)} new records, more than {MAX_NEW}: look at the issues by hand")
        if files:
            subprocess.run([sys.executable, "-I", os.path.join(HERE, "shader_list_merge.py"), os.path.join(root, LIST), *files],
                           check=True)
    after = log_triage.shipped_records(os.path.join(root, LIST))
    summary = {"issues": issues, "added": len(after) - before,
               "pipelines": sum(1 for r in added if r[0] == "G"), "compute": sum(1 for r in added if r[0] == "C"),
               "from": {str(n): len(r) for n, r in sorted(per_issue.items())}, "problems": problems + skipped,
               "total": len(after)}
    return summary


def body_of(s):
    out = [f"Adds {s['added']} shader variant records ({s['pipelines']} pipelines, {s['compute']} compute) that "
           f"players' game logs and `shader_seen.txt` files attached to issues labelled \"{LABEL}\" have and the "
           f"shipped list lacked. The list now has {s['total']} records.",
           "",
           "Records per issue (a record in two issues counts in both):", ""]
    out += [f"- #{n}: {c}" for n, c in s["from"].items()]
    if s["problems"]:
        out += ["", "Not read:", ""] + [f"- {p}" for p in s["problems"]]
    out += ["", "Made by `tools/shader_collect.py` (`.github/workflows/shader_collect.yml`) with "
            "`tools/shader_list_merge.py`; `tools/ci/no_game_data.sh` passed on the list. Each line is hashes and pipeline "
            "state only. A record is not checked against a game's files here: one that matches no shader is skipped by "
            "the prepare step. Nothing is merged automatically."]
    return "\n".join(out) + "\n"


def publish(api, root, summary, dry):
    if not summary["added"]:
        print("no new records: no pull request")
        return
    body = body_of(summary)
    print(body)
    if dry:
        print(f"dry run: would commit {LIST} to {BRANCH} and open or update the pull request")
        return
    base = os.environ["BASE"]
    if not re.fullmatch(r"[A-Za-z0-9._/-]{1,100}", base):
        sys.exit(f"bad base branch {base!r}")
    head, _ = api.call("GET", f"git/ref/heads/{base}")
    sha = head["object"]["sha"]
    ref = f"git/refs/heads/{BRANCH}"
    try:
        api.call("PATCH", ref, {"sha": sha, "force": True})
    except urllib.error.HTTPError as e:
        if e.code != 422: raise  # (422: no such branch yet)
        api.call("POST", "git/refs", {"ref": f"refs/heads/{BRANCH}", "sha": sha})
    cur, _ = api.call("GET", f"contents/{LIST}?ref={urllib.parse.quote(BRANCH)}")
    with open(os.path.join(root, LIST), "rb") as f:
        data = f.read()
    api.call("PUT", f"contents/{LIST}", {"message": f"feat: {summary['added']} shader records from players' logs",
                                         "content": base64.b64encode(data).decode(), "sha": cur["sha"], "branch": BRANCH})
    owner = os.environ["GITHUB_REPOSITORY"].split("/")[0]
    prs, _ = api.call("GET", f"pulls?state=open&head={urllib.parse.quote(owner)}:{urllib.parse.quote(BRANCH)}")
    if prs:
        api.call("PATCH", f"pulls/{prs[0]['number']}", {"title": TITLE, "body": body})
        print(f"updated pull request #{prs[0]['number']}")
    else:
        pr, _ = api.call("POST", "pulls", {"title": TITLE, "head": BRANCH, "base": base, "body": body})
        print(f"opened pull request #{pr['number']}")


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in ("collect", "publish"):
        sys.exit(__doc__)
    root = os.path.abspath(os.path.join(HERE, ".."))
    path = os.environ.get("COLLECT_SUMMARY") or os.path.join(os.environ.get("RUNNER_TEMP", tempfile.gettempdir()), "shader_collect.json")
    dry = os.environ.get("DRY_RUN", "") in ("1", "true")
    api = log_triage.Api(os.environ["GITHUB_TOKEN"], os.environ["GITHUB_REPOSITORY"])
    if sys.argv[1] == "collect":
        summary = collect(api, root)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(summary, f)
        print(json.dumps(summary, indent=1))
    else:
        with open(path, encoding="utf-8") as f:
            publish(api, root, json.load(f), dry)


if __name__ == "__main__":
    main()
