#!/usr/bin/env python3
"""Log triage for GitHub issues: a short summary of the game logs attached to an issue.

Adapted from LAS Recomp's tools/log_triage.py (https://github.com/peiikai/LASRecomp, MIT, Copyright (c) 2026 Peiikai,
see LICENSE-LASRecomp); the log lines it reads are this game's.

.github/workflows/log_triage.yml runs it when an issue or a comment is added or edited. It reads the issue and its
comments through the GitHub API, downloads the attached logs (github.com/user-attachments/files/... links whose file
name is game.log, game-backup-*.log, crash-*.log, RecompLauncher's kirby-forgotten-land-recompiled.log or a
shader_seen.txt, at most MAX_BYTES each), and keeps ONE bot comment per issue up to date with a summary of the newest
logs, plus a "log: crash" / "log: device-lost" / "log: ok" label for the newest log.
Shader variants the shipped list (runtime/nvn/shader_variants.txt) lacks are listed in the comment, folded, and the issue
gets the "log: new shaders" label, so they can be merged into the list (tools/shader_list_merge.py). They come from the
"[shader] record ..." lines of a game.log and from a player's cache/shader_seen.txt.

Everything in an issue and in a log is untrusted data: it is only ever parsed here, never run or handed to a shell.
Excerpts quoted in the comment are cut short, put in code spans, and have file paths reduced to their last part. A
shader record is only quoted when it has exactly the shape of a shipped one (hashes and numbers).

usage:
  log_triage.py parse <log>...      print the comment for local log files (no network)
  log_triage.py issue               the workflow: env GITHUB_TOKEN, GITHUB_REPOSITORY, ISSUE_NUMBER,
                                    DRY_RUN=1 prints the comment instead of posting it and leaves labels alone
Standard library only.
"""
import json
import os
import re
import statistics
import sys
import urllib.error
import urllib.parse
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # (python -I leaves the script's folder off the path)
from shader_list_merge import RECORD_RE, record_of  # noqa: E402  (the shape of a record: one definition, also the merge's)

MARKER = "<!-- kfl-log-triage -->"
MAX_BYTES = 20 * 1024 * 1024  # per log
MAX_LOGS = 4                  # newest logs summarised per comment
LABELS = {"crash": ("log: crash", "b60205", "Newest attached log ends in a crash or hang"),
          "device-lost": ("log: device-lost", "d93f0b", "Newest attached log ends with the GPU device lost"),
          "ok": ("log: ok", "0e8a16", "Newest attached log ends with a clean exit")}
SHADER_LABEL = ("log: new shaders", "5319e7", "Attached log has shader variants the shipped list lacks")
MAX_RECORD_CHARS = 30000      # shader records quoted per comment (GitHub stops a comment at 65536 characters)
SHIPPED_LIST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "runtime", "nvn", "shader_variants.txt")
ATTACH_RE = re.compile(r"https://github\.com/user-attachments/files/\d+/[A-Za-z0-9._%+-]+")
REDIRECT_HOSTS = ("objects.githubusercontent.com",)  # (where github.com/user-attachments/files/... redirects)
# the files players attach: game/game.h GAME_LOG ("game", and its game-backup-<filetime>-<pid>), the launcher's copy of
# a crashed run (launcher/src-tauri/src/main.rs), RecompLauncher's log of the game's output, the player's shader list
ATTACH_NAME_RE = re.compile(r"game(-backup-[0-9a-f]+-\d+)?\.(log|txt)|crash-\d{8}-\d{6}\.log"
                            r"|kirby-forgotten-land-recompiled[\w.-]*\.(log|txt)|shader_seen[\w.-]*\.txt")


# ------------------------------------------------------------------ parsing a log

def clean(s, n=160):
    """A log excerpt made safe to quote: paths cut to their last part, no backticks / control characters, short."""
    s = re.sub(r"[A-Za-z]:[\\/][^\s:;,)\"']*[\\/]([^\s\\/:;,)\"']+)", r"\1", s)        # C:\Users\x\...\file
    s = re.sub(r"(?<![\w.])/(?:[^\s/:;,)\"']+/)+([^\s/:;,)\"']+)", r"\1", s)           # /home/x/.../file
    s = "".join(c if c.isprintable() else " " for c in s).replace("`", "'").strip()
    return s if len(s) <= n else s[:n - 3] + "..."


def code(s, n=160):
    return "`" + clean(s, n) + "`"


FPS_RE = re.compile(r"^\[vk\] t=([\d.]+) frame (\d+) ([\d.]+) fps max (\d+)ms(?: level (\S+))?")
# runtime/main.c log_startup_info: "[info] Forgotten Land Recomp v0.3.2, Windows 11 (10.0.26300, x64), 11th Gen Intel(R)
# Core(TM) i7-11800H 8 cores / 16 threads, 31.8 GB RAM, zip" - the build; the OS, CPU and RAM; the package when known
INFO_RE = re.compile(r"^\[info\] Forgotten Land Recomp ([^\s,]+), (.+?(?:GB RAM|RAM unknown))(?:, (.+))?$")
OLD_INFO_RE = re.compile(r"^\[paths\] Forgotten Land Recomp ([^\s,]+), data: ")  # (0.2.0 and earlier: no [info] line)
MS_RE = re.compile(r"([\d.]+)\s*ms")
# runtime/services/fs.c, the game files check: files that aren't the game's (the first 20 named), the game's files
# missing or of another size, and a byte total other than the version's
ROMFS_RE = re.compile(r"^\[fs\] romfs: (?:not one of the game's files: (.+) \(\d+ bytes\)|(\d+) more files "
                      r"that aren't the game's|(\d+) of the game's files missing or changed|(\d+) bytes in \d+ files, "
                      r"this version's are (\d+): damaged or changed game files)$")

# notable lines: (label, regex); each is reported once, with its count and first line
NOTABLE = [
    ("mods replace game files", re.compile(r"^\[fs\] mods: \d+ romfs files replaced")),
    ("heap as driver memory failed", re.compile(r"heap as driver memory: (?!.*shared as before$).*(fail|not allowed|no way|can't|no cached|would be)")),
    ("heap as driver memory fell back", re.compile(r"heap as driver memory: .*shared as before")),
    ("guest heap imported in chunks (not enough GPU memory for one import)", re.compile(r"\[vk\] guest heap: .*(ran out of memory|imported in chunks)")),
    ("GPU device fault", re.compile(r"\[vk\] device fault")),
    ("shaders of the lists not found in the game files, or failed to prepare", re.compile(r"\[shader\] prepared \d+ \+ \d+ seen before, not found [1-9]|\[shader\] prepared .*\(([1-9]\d*) failed")),
    ("shader translation failed", re.compile(r"translate failed|pipeline failed|\[shader\] stale program")),
    ("slow shader builds (stutter)", re.compile(r"\[shader\] slow build")),
    ("settings file could not be written", re.compile(r"\[settings\] cannot write")),
    ("developer overrides (LARECOMP_*) in use", re.compile(r"\(LARECOMP_[A-Z_]+\)")),
]


def shipped_records(path=SHIPPED_LIST):
    """the records of the shipped list, or None when the file isn't there"""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return {ln.strip() for ln in f if RECORD_RE.fullmatch(ln.strip())}
    except OSError:
        return None


def parse(text, shipped=frozenset()):
    """text of one attached file -> what it says; shader records are each kept once and only when `shipped` lacks them"""
    lines = text.splitlines()
    r = {"build": None, "game": None, "system": None, "gpu": None, "gpus": [], "api": None, "driver": None,
         "settings": None, "swapchain": None, "integrated": False, "fps": [], "slow": [], "notable": {},
         "records": [], "seen_file": False, "end": None, "end_line": None, "crash_head": None, "romfs": None}
    romfs = {"odd": [], "more": 0, "missing": 0, "bytes": None}
    records = {}
    fatal = exc = None
    devlost = hang = closed = False
    for i, ln in enumerate(lines):
        if rec := record_of(ln):
            if rec not in shipped: records[rec] = None
            continue
        if ln.startswith("# shader_seen"):
            r["seen_file"] = True
        elif m := INFO_RE.match(ln):  # (the build's own name)
            r["build"] = m.group(1) + (f" ({m.group(3)})" if m.group(3) else "")
            r["system"] = m.group(2)
        elif (m := OLD_INFO_RE.match(ln)) and not r["build"]:
            r["build"] = m.group(1)
        elif m := re.match(r"^\[game\] version (\S+) ", ln):
            r["game"] = m.group(1)
        elif m := re.match(r"^\[vk\] gpu \d+: (.+) \(api ([\d.]+)\)", ln):
            r["gpus"].append(m.group(1))
        elif m := re.match(r"^\[vk\] using (.+)", ln):
            r["gpu"] = m.group(1)
            for g in lines[:i]:
                mm = re.match(r"^\[vk\] gpu \d+: (.+) \(api ([\d.]+)\)", g)
                if mm and mm.group(1) == r["gpu"]: r["api"] = mm.group(2)
        elif m := re.match(r"^\[vk\] driver (.+?)(?:; |$)", ln):  # ("[vk] driver NVIDIA 610.62; graphics pipeline library: ...")
            r["driver"] = m.group(1)
        elif ln.startswith("[settings] ") and " GPU: upscaler " in ln:
            r["settings"] = ln.split(" GPU: ", 1)[1]
            r["integrated"] = ln.startswith("[settings] integrated")
        elif m := re.match(r"^\[vk\] swapchain (\d+x\d+), (\d+) images, (\w+)", ln):
            r["swapchain"] = f"{m.group(1)}, {m.group(3)}"
        elif m := FPS_RE.match(ln):
            r["fps"].append((float(m.group(1)), float(m.group(3)), int(m.group(4)), m.group(5) or "-"))
        elif ln.startswith("[slow] frame "):
            r["slow"].append(ln)
        elif ln.startswith("FATAL: ") and not fatal:
            fatal = ln[7:]
            if "device lost" in ln or re.search(r"failed: -4 \(", ln): devlost = True
        elif ln.startswith("*** ") and not exc:
            exc = ln[4:]
        elif ln.startswith("[crash] "):
            r["crash_head"] = ln[8:]
        elif ln.startswith("[vk] last GPU operations before the device loss") or "VK_ERROR_DEVICE_LOST" in ln:
            devlost = True
        elif ln.startswith("[hang] ====="):
            hang = True
        elif ln.strip() == "window closed" or ln.startswith("[menu] quit:"):
            closed = True
        elif m := ROMFS_RE.match(ln.rstrip()):
            if m.group(1) is not None:
                if m.group(1) not in romfs["odd"]: romfs["odd"].append(m.group(1))
            elif m.group(2): romfs["more"] = int(m.group(2))
            elif m.group(3): romfs["missing"] = int(m.group(3))
            else: romfs["bytes"] = (int(m.group(4)), int(m.group(5)))
        for label, rx in NOTABLE:
            if rx.search(ln):
                n, first = r["notable"].get(label, (0, ln))
                r["notable"][label] = (n + 1, first)
    r["records"] = sorted(records)
    if romfs["odd"] or romfs["more"] or romfs["missing"] or romfs["bytes"]: r["romfs"] = romfs
    if r["integrated"] or any("integrated GPU" in x and "not an integrated GPU" not in x for x in lines[:200]):
        r["integrated"] = True
    if devlost:
        r["end"], r["end_line"] = "device-lost", fatal or "the GPU device was lost"
    elif fatal or exc or r["crash_head"]:
        r["end"], r["end_line"] = "crash", fatal or exc or r["crash_head"]
    elif hang:
        r["end"], r["end_line"] = "hang", next((x for x in lines if x.startswith("[hang] =====") and x.strip() != "[hang] ====="), None)
    elif closed:
        r["end"] = "ok"
    else:
        r["end"] = "unknown"
    return r


def fps_summary(fps):
    if not fps: return None
    vals = [f for _, f, _, _ in fps]
    span = fps[-1][0] - fps[0][0]
    worst_ms = max(m for _, _, m, _ in fps)
    by_level = {}
    for _, f, _, lv in fps: by_level.setdefault(lv, []).append(f)
    lows = sorted(((statistics.median(v), lv) for lv, v in by_level.items() if len(v) >= 2 and lv != "-"))[:2]
    s = (f"{len(fps)} samples over {span:.0f} s: median {statistics.median(vals):.1f} fps, lowest {min(vals):.1f} fps, "
         f"longest frame {worst_ms} ms")
    if len(by_level) > 1 and lows:
        s += "; slowest areas: " + ", ".join(f"{code(lv, 40)} {f:.1f} fps" for f, lv in lows)
    return s


def records_line(n, g):
    return f"{n} the shipped list lacks ({g} pipelines, {n - g} compute)"


def summarise(r):
    if r["seen_file"]:
        g = sum(1 for x in r["records"] if x[0] == "G")
        return "- **Shader list file:** " + (records_line(len(r["records"]), g) if r["records"] else "no record the shipped list lacks")
    out = []
    out.append(f"- **Build:** {code(r['build']) if r['build'] else 'not in the log'}"
               + (f", game version {code(r['game'], 20)}" if r["game"] else ""))
    out.append("- **System:** " + (code(r["system"]) if r["system"] else "not in the log"))
    if r["gpu"]:
        g = code(r["gpu"], 80) + (" (integrated)" if r["integrated"] else "")
        if r["api"]: g += f", Vulkan {r['api']}"
        if r["driver"]: g += f", driver {code(r['driver'], 60)}"
        if len(r["gpus"]) > 1: g += f" ({len(r['gpus'])} GPUs found)"
        out.append(f"- **GPU:** {g}")
    else:
        out.append("- **GPU:** not in the log")
    if r["romfs"]:
        c, gf = r["romfs"], []
        if c["missing"]: gf.append(f"{c['missing']} of the game's files missing or changed")
        odd = len(c["odd"]) + c["more"]
        if odd:
            gf.append(f"{odd} file{' that isn' if odd == 1 else 's that aren'}'t the game's"
                      + (f" ({'first: ' if odd > 1 else ''}{code(c['odd'][0], 80)})" if c["odd"] else ""))
        if c["bytes"]: gf.append(f"{c['bytes'][0]} bytes, this version's are {c['bytes'][1]}")
        # (only extra files: not a damaged copy)
        out.append("- **Game files:** " + ("damaged or changed: " if c["missing"] or c["bytes"] else "") + "; ".join(gf))
    st = []
    if r["settings"]: st.append(code(r["settings"], 120))
    if r["swapchain"]: st.append(f"window {r['swapchain']}")
    out.append("- **Graphics settings:** " + ("; ".join(st) if st else "not in the log"))
    end = {"ok": "clean exit (window closed, or quit from the menu)",
           "crash": "crash" + (f" ({clean(r['crash_head'], 80)})" if r["crash_head"] else "") + f": {code(r['end_line'] or '')}",
           "device-lost": f"the GPU device was lost: {code(r['end_line'] or '')}",
           "hang": f"hang (the hang report is in the log){': ' + code(r['end_line']) if r['end_line'] else ''}",
           "unknown": "the log stops without an exit line (closed from outside, frozen, or still running when copied)"}[r["end"]]
    out.append(f"- **How the run ended:** {end}")
    fs = fps_summary(r["fps"])
    out.append(f"- **Frame rate:** {fs or 'no frame rate lines'}")
    if r["slow"]:
        def ms(x):
            m = MS_RE.search(x)
            return float(m.group(1)) if m else 0.0
        worst = max(r["slow"], key=ms)
        out.append(f"- **Slow frames:** {len(r['slow'])} `[slow]` lines; worst: {code(worst, 140)}")
    if r["records"]:
        g = sum(1 for x in r["records"] if x[0] == "G")
        out.append(f"- **New shader variants:** {records_line(len(r['records']), g)}")
    if r["notable"]:
        out.append("- **Notable lines:**")
        for label, (n, first) in r["notable"].items():
            out.append(f"  - {label}{f' ({n}x)' if n > 1 else ''}: {code(first, 140)}")
    return "\n".join(out)


def shader_records(entries):
    """every shader record in the summarised logs, once, sorted (the shipped list's order)"""
    return sorted({x for _, r in entries if isinstance(r, dict) for x in r["records"]})


def render(entries, total):
    """entries: newest first, (source description, parse result or error string)"""
    out = [MARKER,
           "This is an automatic summary of the log files attached to this issue. It only reads the logs, "
           "so a person will still look at the details.",
           "",
           "For a new report, please attach `game.log` from the install folder (it always holds the latest run; "
           "`game-backup-*.log` there are the earlier runs). With RecompLauncher, attach "
           "`kirby-forgotten-land-recompiled.log` from `%APPDATA%\\com.recomplauncher.app\\game-output\\` instead. "
           "For stutter, also attach `cache\\shader_seen.txt`."]
    for k, (src, r) in enumerate(entries):
        out += ["", f"#### {'Newest log' if k == 0 else 'Earlier log'}: {src}"]
        out.append(summarise(r) if isinstance(r, dict) else f"- Could not read this log: {clean(r, 120)}")
    recs = shader_records(entries)
    if recs:
        shown, size = [], 0
        for x in recs:
            size += len(x) + 1
            if size > MAX_RECORD_CHARS: break
            shown.append(x)
        out += ["", "<details><summary>Shader variants the shipped list lacks (" + str(len(recs)) + ")</summary>", "",
                "```"] + shown + ["```", ""]
        if len(recs) > len(shown): out.append(f"({len(recs) - len(shown)} more in the attached files.)")
        out.append("</details>")
    if total > len(entries):
        out += ["", f"({total - len(entries)} earlier log{'s' if total - len(entries) > 1 else ''} on this issue not summarised.)"]
    return "\n".join(out) + "\n"


def newest_end(entries):
    """the label's key for the newest attached log (a shader_seen.txt says nothing about how a run ended)"""
    for _, r in entries:
        if isinstance(r, dict) and not r["seen_file"]:
            return {"crash": "crash", "hang": "crash", "device-lost": "device-lost", "ok": "ok"}.get(r["end"])
    return None


# ------------------------------------------------------------------ GitHub

class Api:
    def __init__(self, token, repo):
        self.token, self.repo = token, repo

    def call(self, method, path, body=None):
        url = path if path.startswith("https://") else f"https://api.github.com/repos/{self.repo}/{path}"
        req = urllib.request.Request(url, method=method, data=json.dumps(body).encode() if body is not None else None,
                                     headers={"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
                                              "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "kfl-log-triage"})
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = resp.read()
            link = resp.headers.get("Link", "")
        return (json.loads(data) if data else None), link

    def pages(self, path):
        out, url = [], f"https://api.github.com/repos/{self.repo}/{path}"
        for _ in range(50):
            data, link = self.call("GET", url)
            out += data
            m = re.search(r'<([^>]+)>;\s*rel="next"', link)
            if not m: break
            url = m.group(1)
        return out


class _Redirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code_, msg, headers, newurl):
        host = urllib.parse.urlparse(newurl).hostname or ""
        if urllib.parse.urlparse(newurl).scheme != "https" or host not in REDIRECT_HOSTS:
            raise urllib.error.HTTPError(newurl, code_, f"redirect to {host} not allowed", headers, fp)
        return super().redirect_request(req, fp, code_, msg, headers, newurl)


def download(url):
    if not ATTACH_RE.fullmatch(url): raise ValueError("not a GitHub attachment link")
    opener = urllib.request.build_opener(_Redirects)
    req = urllib.request.Request(url, headers={"User-Agent": "kfl-log-triage"})  # (no token: public files)
    with opener.open(req, timeout=60) as resp:
        if int(resp.headers.get("Content-Length") or 0) > MAX_BYTES: raise ValueError("log larger than 20 MB")
        data = resp.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES: raise ValueError("log larger than 20 MB")
    return data.decode("utf-8", "replace")


def attachments(text):
    """attachment links of the game's logs and shader lists in a body, in order, without repeats"""
    seen, out = set(), []
    for u in ATTACH_RE.findall(text or ""):
        name = urllib.parse.unquote(u.rsplit("/", 1)[1]).lower()
        if ATTACH_NAME_RE.fullmatch(name) and u not in seen:
            seen.add(u)
            out.append(u)
    return out


def run_issue():
    token, repo = os.environ["GITHUB_TOKEN"], os.environ["GITHUB_REPOSITORY"]
    num = os.environ.get("ISSUE_NUMBER", "")
    if not re.fullmatch(r"[1-9]\d{0,6}", num): sys.exit(f"bad issue number {num!r}")
    dry = os.environ.get("DRY_RUN", "") in ("1", "true")
    api = Api(token, repo)
    issue, _ = api.call("GET", f"issues/{num}")
    if issue.get("pull_request"): print("a pull request: skipped"); return
    comments = api.pages(f"issues/{num}/comments?per_page=100")
    found = [(u, "the issue text", issue["html_url"]) for u in attachments(issue.get("body"))]
    mine = None
    for c in comments:
        if (c.get("user") or {}).get("type") == "Bot":
            if MARKER in (c.get("body") or "") and c["user"]["login"] == "github-actions[bot]": mine = c
            continue
        found += [(u, f"comment from {c['created_at'][:10]}", c["html_url"]) for u in attachments(c.get("body"))]
    seen = set()  # (a quote reply repeats the quoted comment's links: each log once, where it was attached)
    found = [f for f in found if f[0] not in seen and not seen.add(f[0])]
    if not found: print("no game log attached: nothing to do"); return
    shipped = shipped_records()
    if shipped is None:
        print(f"{SHIPPED_LIST} not found: shader records are not checked against the shipped list")
        shipped = frozenset()
    newest = list(reversed(found))[:MAX_LOGS]
    entries = []
    for u, where, link in newest:
        name = urllib.parse.unquote(u.rsplit("/", 1)[1])
        src = f"[{clean(name, 60)}]({u}) in [{where}]({link})"
        try:
            entries.append((src, parse(download(u), shipped)))
        except Exception as e:  # (a bad download is reported in the comment, never fatal)
            entries.append((src, f"{type(e).__name__}: {e}"))
    body = render(entries, len(found))
    end = newest_end(entries)
    label = LABELS[end][0] if end else None
    print(body)
    if shader_records(entries): print(f"label: {SHADER_LABEL[0]}")
    print(f"label: {label or '(none)'}; existing bot comment: {mine['html_url'] if mine else '(none)'}")
    if dry:
        print("dry run: nothing posted")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as f:
                f.write(f"Dry run for issue #{num} (label: {label or 'none'}):\n\n{body}\n")
        return
    if mine:
        if mine["body"] != body: api.call("PATCH", f"issues/comments/{mine['id']}", {"body": body})
    else:
        api.call("POST", f"issues/{num}/comments", {"body": body})
    have = {l["name"] for l in issue.get("labels", [])}
    for name, color, desc in LABELS.values():
        if name in have and name != label:
            api.call("DELETE", f"issues/{num}/labels/{urllib.parse.quote(name)}")
    if label and label not in have:
        add_label(api, num, LABELS[end])
    if shader_records(entries) and SHADER_LABEL[0] not in have:  # (only added, never removed: the issue watch keys on it)
        add_label(api, num, SHADER_LABEL)


def add_label(api, num, spec):
    name, color, desc = spec
    try:
        api.call("POST", "labels", {"name": name, "color": color, "description": desc})
    except urllib.error.HTTPError as e:
        if e.code != 422: raise  # (422: it exists already)
    api.call("POST", f"issues/{num}/labels", {"labels": [name]})


def main():
    if len(sys.argv) >= 3 and sys.argv[1] == "parse":
        shipped = shipped_records() or frozenset()
        entries = []
        for p in reversed(sys.argv[2:]):
            with open(p, encoding="utf-8", errors="replace") as f:
                entries.append((os.path.basename(p), parse(f.read(), shipped)))
        sys.stdout.write(render(entries[:MAX_LOGS], len(entries)))
        if shader_records(entries[:MAX_LOGS]): print("label:", SHADER_LABEL[0])
        end = newest_end(entries)
        print("label:", LABELS[end][0] if end else "(none)")
    elif len(sys.argv) == 2 and sys.argv[1] == "issue":
        run_issue()
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
