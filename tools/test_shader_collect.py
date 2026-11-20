"""tools/shader_collect.py and tools/shader_list_merge.py on fixtures: the labelled issues' attachments are gathered, merged
into the list (only lines of the record shape get in, the same issues always give the same file), and the pull request
is made from numbers only. The GitHub API and the downloads are replaced by a fixture; no game data, no network.

Usage: test_shader_collect.py
"""
import contextlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import urllib.error
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import log_triage as lt  # noqa: E402
import shader_collect as sc  # noqa: E402
import shader_list_merge as merge  # noqa: E402
from test_log_triage import log, rec  # noqa: E402

ATT = "https://github.com/user-attachments/files/%d/%s"


class FakeApi:
    def __init__(self, issues, comments):
        self.issues, self.comments, self.calls = issues, comments, []

    def pages(self, path):
        if path.startswith("issues?labels="):
            assert "log%3A%20new%20shaders" in path
            return self.issues
        n = int(path.split("/")[1])
        return self.comments.get(n, [])


def issue(n, *urls, pr=False):
    d = {"number": n, "body": " ".join(f"[f]({u})" for u in urls)}
    if pr: d["pull_request"] = {}
    return d


def comment(*urls, bot=False):
    return {"user": {"type": "Bot" if bot else "User"}, "body": " ".join(urls)}


class Merge(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir)

    def write(self, name, *lines):
        p = os.path.join(self.dir, name)
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(lines) + "\n")
        return p

    def test_only_record_lines_get_in_and_the_list_only_grows(self):
        out = self.write("out.txt", rec(5), rec(6, "C"))
        a = self.write("a.log", "[shader] record " + rec(1), "[shader] record " + rec(1) + " extra", "[shader] record G zz",
                       "# shader_seen game 1", "plain text", rec(2, "C"), rec(9) + "\n" + "G " + "a" * 5000)
        b = self.write("b.txt", "# shader_seen game 2", rec(1), rec(3), "C cs=1 l=1,1,1", "x [shader] record " + rec(4))
        with mock.patch.object(sys, "argv", ["m", out, a, b]), contextlib.redirect_stdout(io.StringIO()) as o:
            merge.main()
        with open(out, encoding="utf-8") as f:
            got = f.read().splitlines()
        self.assertEqual(got, sorted([rec(5), rec(6, "C"), rec(1), rec(2, "C"), rec(9), rec(3), rec(4)]))
        self.assertIn("7 records", o.getvalue())
        self.assertIn("5 new", o.getvalue())
        self.assertIn("malformed lines skipped", o.getvalue())

    def test_a_bad_line_in_the_list_itself_is_dropped_on_the_next_merge(self):
        out = self.write("out.txt", rec(1), "G bad line")
        with mock.patch.object(sys, "argv", ["m", out, self.write("a.txt", rec(2))]), contextlib.redirect_stdout(io.StringIO()):
            merge.main()
        with open(out, encoding="utf-8") as f:
            self.assertEqual(f.read().splitlines(), sorted([rec(1), rec(2)]))


class Collect(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.root)
        self.shipped = [rec(100), rec(101, "C")]
        self.reset()

    def reset(self):
        p = os.path.join(self.root, sc.LIST)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            f.write("\n".join(sorted(self.shipped)) + "\n")

    def files(self):
        return {
            ATT % (1, "game.log"): log("[info] x", f"[shader] record {rec(1)}", f"[shader] record {rec(100)}", f"[shader] record {rec(2)}"),
            ATT % (2, "shader_seen.txt"): log("# shader_seen game 1", rec(2), rec(3, "C"), rec(101, "C")),
            ATT % (3, "crash-20261116-092150.log"): log("FATAL: x", f"[shader] record {rec(4)} injected"),
            ATT % (4, "game.log"): log(f"[shader] record {rec(5)}"),
            ATT % (5, "game.log"): None,  # a download that fails
        }

    def api(self, reverse=False):
        issues = [issue(7, ATT % (1, "game.log"), ATT % (3, "crash-20261116-092150.log"), ATT % (5, "game.log")),
                  issue(3, ATT % (2, "shader_seen.txt")), issue(9, ATT % (4, "game.log"), pr=True),
                  issue(5, ATT % (9, "setup.log"))]
        comments = {3: [comment(ATT % (1, "game.log")), comment(ATT % (4, "game.log"), bot=True)]}
        return FakeApi(issues[::-1] if reverse else issues, comments)

    def run_collect(self, **kw):
        files = self.files()

        def fake_download(url):
            if files[url] is None:
                raise urllib.error.HTTPError(url, 404, "Not Found", {}, None)
            return files[url]
        with mock.patch.object(lt, "download", fake_download), contextlib.redirect_stdout(io.StringIO()):
            return sc.collect(self.api(**kw), self.root)

    def list_text(self):
        with open(os.path.join(self.root, sc.LIST), encoding="utf-8") as f:
            return f.read()

    def test_records_of_the_labelled_issues_are_merged_once_and_counted(self):
        s = self.run_collect()
        self.assertEqual(self.list_text().splitlines(), sorted(self.shipped + [rec(1), rec(2), rec(3, "C")]))
        self.assertEqual((s["added"], s["pipelines"], s["compute"], s["total"]), (3, 2, 1, 5))
        self.assertEqual(s["issues"], [3, 5, 7])  # (the pull request is not an issue; the bot's comment is not read)
        self.assertEqual(s["from"], {"3": 3})  # (#3's comment has #7's log: each url once, at the lowest issue number)
        self.assertEqual(s["problems"], ["#7: HTTPError"])

    def test_the_same_issues_give_the_same_file(self):
        self.run_collect()
        first = self.list_text()
        self.reset()
        self.run_collect(reverse=True)
        self.assertEqual(first, self.list_text())

    def test_nothing_new_leaves_the_list_as_it_was(self):
        self.shipped += [rec(1), rec(2), rec(3, "C")]
        self.reset()
        before = self.list_text()
        s = self.run_collect()
        self.assertEqual(s["added"], 0)
        self.assertEqual(before, self.list_text())

    def test_too_many_new_records_stop_the_run(self):
        with mock.patch.object(sc, "MAX_NEW", 2), self.assertRaises(SystemExit):
            self.run_collect()

    def test_the_pull_request_text_is_numbers_only(self):
        s = self.run_collect()
        body = sc.body_of(s)
        self.assertIn("Adds 3 shader variant records (2 pipelines, 1 compute)", body)
        self.assertIn("- #3: 3", body)
        self.assertIn("- #7: HTTPError", body)
        self.assertIn("Nothing is merged automatically", body)
        self.assertNotIn("LAS", body)


class Publish(unittest.TestCase):
    SUMMARY = {"issues": [3], "added": 2, "pipelines": 2, "compute": 0, "from": {"3": 2}, "problems": [], "total": 9}

    def api(self, prs, branch_exists):
        api = mock.Mock()
        calls = []

        def call(method, path, body=None):
            calls.append((method, path.split("?")[0]))
            if method == "GET" and path.startswith("git/ref/"): return {"object": {"sha": "abc"}}, ""
            if method == "PATCH" and path.startswith("git/refs/") and not branch_exists:
                raise urllib.error.HTTPError(path, 422, "no ref", {}, None)
            if method == "GET" and path.startswith("contents/"): return {"sha": "blob"}, ""
            if method == "GET" and path.startswith("pulls?"): return prs, ""
            if method == "POST" and path == "pulls": return {"number": 12}, ""
            return {}, ""
        api.call = call
        return api, calls

    def run_publish(self, summary, dry, **kw):
        api, calls = self.api(**kw)
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root)
        os.makedirs(os.path.join(root, "runtime", "nvn"))
        open(os.path.join(root, sc.LIST), "w").write(rec(1) + "\n")
        env = {"BASE": "main", "GITHUB_REPOSITORY": "owner/repo"}
        with mock.patch.dict(os.environ, env), contextlib.redirect_stdout(io.StringIO()) as o:
            sc.publish(api, root, summary, dry)
        return calls, o.getvalue()

    def test_no_new_records_make_no_pull_request(self):
        calls, out = self.run_publish(dict(self.SUMMARY, added=0), False, prs=[], branch_exists=True)
        self.assertEqual(calls, [])

    def test_a_dry_run_changes_nothing(self):
        calls, out = self.run_publish(self.SUMMARY, True, prs=[], branch_exists=True)
        self.assertEqual(calls, [])
        self.assertIn("dry run", out)

    def test_the_branch_is_reset_and_a_new_pull_request_opened(self):
        calls, out = self.run_publish(self.SUMMARY, False, prs=[], branch_exists=False)
        self.assertEqual(calls, [("GET", "git/ref/heads/main"), ("PATCH", "git/refs/heads/shader-list/collect"), ("POST", "git/refs"),
                                 ("GET", "contents/" + sc.LIST), ("PUT", "contents/" + sc.LIST), ("GET", "pulls"), ("POST", "pulls")])

    def test_an_open_pull_request_is_updated_not_duplicated(self):
        calls, out = self.run_publish(self.SUMMARY, False, prs=[{"number": 4}], branch_exists=True)
        self.assertEqual(calls[-1], ("PATCH", "pulls/4"))
        self.assertNotIn(("POST", "pulls"), calls)


class Cli(unittest.TestCase):
    def test_the_merge_runs_isolated_as_the_collector_starts_it(self):
        with tempfile.TemporaryDirectory() as d:
            out, a = os.path.join(d, "out.txt"), os.path.join(d, "a.txt")
            open(a, "w").write(rec(1) + "\nbad\n")
            r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "shader_list_merge.py"), out, a], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(open(out).read(), rec(1) + "\n")


if __name__ == "__main__":
    unittest.main(verbosity=1)
