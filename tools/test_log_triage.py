"""tools/log_triage.py on synthetic log text: records extracted from a game.log and a shader_seen.txt, duplicates and
records the shipped list has dropped, lines that only look like a record refused, the size caps (a comment under
GitHub's limit, a download over MAX_BYTES refused), the attachment host allow-list. No game data, no network.

Usage: test_log_triage.py
"""
import io
import os
import sys
import unittest
import urllib.error
import urllib.request
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import log_triage as lt  # noqa: E402


def rec(i, kind="G"):
    if kind == "C":
        return f"C cs={i:016x} l=64,1,1"
    return f"G vs={i:016x} fs={i + 1:016x} f=3 a=16 s=8/0 c=38:38 d=0 n=1 m=f b=1 B=0005060506010100 k=0 t=2 p=1"


def log(*lines):
    return "\n".join(lines) + "\n"


INFO = ("[info] Forgotten Land Recomp v0.3.3, Windows 11 (10.0.26300, x64), Test CPU 8 cores / 16 threads, 32.0 GB RAM, zip",
        "[game] version 1.0.0 (main build ID 0000000000000000, address table v1_0_0)",
        "[vk] gpu 0: Test Integrated (api 1.2.1)", "[vk] gpu 1: Test Discrete GPU (api 1.4.3)", "[vk] using Test Discrete GPU",
        "[vk] driver TestVendor 1.2.3; graphics pipeline library: yes",
        "[settings] discrete / other GPU: upscaler fsr1 (chosen), resolution match (default), anti-aliasing off",
        "[vk] swapchain 1920x1080, 3 images, mailbox")


class Records(unittest.TestCase):
    def test_log_lines_and_seen_file_lines_are_extracted(self):
        r = lt.parse(log(*INFO, f"[shader] record {rec(1)}", "[shader] first visit 3: program x y",
                         f"[shader] record {rec(2, 'C')}"))
        self.assertEqual(r["records"], sorted([rec(1), rec(2, "C")]))
        seen = lt.parse(log("# shader_seen game 0123", rec(3), rec(4, "C")))
        self.assertTrue(seen["seen_file"])
        self.assertEqual(seen["records"], sorted([rec(3), rec(4, "C")]))

    def test_duplicates_are_kept_once(self):
        r = lt.parse(log(f"[shader] record {rec(1)}", f"[shader] record {rec(1)}", rec(1)))
        self.assertEqual(r["records"], [rec(1)])
        entries = [("a", lt.parse(log(rec(1), rec(2)))), ("b", lt.parse(log(rec(2), rec(3))))]
        self.assertEqual(lt.shader_records(entries), [rec(1), rec(2), rec(3)])

    def test_records_the_shipped_list_has_are_dropped(self):
        r = lt.parse(log(rec(1), rec(2), rec(3, "C")), shipped={rec(2), rec(3, "C")})
        self.assertEqual(r["records"], [rec(1)])
        self.assertIn("no record the shipped list lacks", lt.summarise(lt.parse(log("# shader_seen game 1", rec(2)), {rec(2)})))

    def test_a_line_that_only_looks_like_a_record_is_refused(self):
        bad = [rec(1) + " extra", rec(1).replace("vs=", "vs=zz"), rec(1)[:-4], "G " + "a" * 5000, rec(1) + "```",
               "G vs=0000000000000001 fs=0000000000000002 f=3 a=16 s=8/0 c=38:38 d=0 n=1 m=f b=1 B= k=0 t=2 p=1 @everyone",
               "C cs=1 l=1,1,1"]
        r = lt.parse(log(*[f"[shader] record {b}" for b in bad], *bad))
        self.assertEqual(r["records"], [])

    def test_the_shipped_list_is_all_records(self):
        path = os.path.join(HERE, "..", "runtime", "nvn", "shader_variants.txt")
        with open(path, encoding="utf-8") as f:
            n = sum(1 for ln in f if ln.strip())
        self.assertEqual(len(lt.shipped_records(path)), n)


class Caps(unittest.TestCase):
    def test_a_comment_stays_under_githubs_limit_however_many_records(self):
        r = lt.parse(log(*[f"[shader] record {rec(i)}" for i in range(20000)]))
        body = lt.render([("big.log", r)] * 4, 40)
        self.assertLess(len(body), 65536)
        self.assertIn("more in the attached files", body)
        self.assertIn("(20000)", body)

    def test_a_download_over_the_cap_is_refused(self):
        class Resp(io.BytesIO):
            def __init__(self, data, length):
                super().__init__(data)
                self.headers = {"Content-Length": str(length)}
            def __enter__(self): return self
            def __exit__(self, *a): pass
        url = "https://github.com/user-attachments/files/1/game.log"
        for data, length in ((b"x", lt.MAX_BYTES + 1), (b"x" * (lt.MAX_BYTES + 1), 0)):
            opener = mock.Mock()
            opener.open.return_value = Resp(data, length)
            with mock.patch.object(urllib.request, "build_opener", return_value=opener):
                with self.assertRaises(ValueError):
                    lt.download(url)
        opener.open.return_value = Resp(b"fine", 4)
        with mock.patch.object(urllib.request, "build_opener", return_value=opener):
            self.assertEqual(lt.download(url), "fine")

    def test_only_githubs_attachment_host_is_downloaded_from(self):
        for url in ("https://evil.example/user-attachments/files/1/game.log", "http://github.com/user-attachments/files/1/game.log",
                    "https://github.com/user-attachments/files/1/game.log@evil.example", "file:///etc/passwd",
                    "https://github.com/user-attachments/files/1/game.log\n"):
            with self.assertRaises(ValueError):
                lt.download(url)
        h = lt._Redirects()
        req = urllib.request.Request("https://github.com/user-attachments/files/1/game.log")
        for new in ("https://evil.example/x", "http://objects.githubusercontent.com/x", "https://github.com.evil.example/x"):
            with self.assertRaises(urllib.error.HTTPError):
                h.redirect_request(req, io.BytesIO(), 302, "Found", {}, new)
        self.assertIsNotNone(h.redirect_request(req, io.BytesIO(), 302, "Found", {}, "https://objects.githubusercontent.com/x?y=1"))

    def test_only_the_games_files_are_taken_from_a_body(self):
        base = "https://github.com/user-attachments/files/%d/%s"
        names = ["game.log", "game-backup-1d9a8b7c6d5e4f3-1234.log", "crash-20261116-092150.log",
                 "kirby-forgotten-land-recompiled.log", "shader_seen.txt", "Game.log"]
        other = ["setup.log", "notes.txt", "game.exe", "gameplay.log", "game.log.exe", "crash-1.log"]
        body = " ".join(f"[x]({base % (i, n)})" for i, n in enumerate(names + other)) + " https://evil.example/files/1/game.log"
        body += f" {base % (0, 'game.log')}"
        got = lt.attachments(body)
        self.assertEqual([u.rsplit("/", 1)[1] for u in got], names)
        self.assertEqual(lt.attachments(None), [])


class Summary(unittest.TestCase):
    def test_the_run_is_described_from_its_lines(self):
        fps = ["[vk] t=%d.0 frame %d 30.0 fps max 40ms level -" % (10 + i, 300 * i) for i in range(5)]
        s = lt.summarise(lt.parse(log(*INFO, *fps, "[menu] quit: back to the start screen")))
        for part in ("v0.3.3 (zip)", "game version `1.0.0`", "Test Discrete GPU", "Vulkan 1.4.3", "(2 GPUs found)", "upscaler fsr1",
                     "window 1920x1080, mailbox", "clean exit", "5 samples over 4 s: median 30.0 fps"):
            self.assertIn(part, s)
        self.assertNotIn("LAS", s)

    def test_a_crash_a_lost_device_and_an_unfinished_run_are_told_apart(self):
        self.assertEqual(lt.parse(log(*INFO, "FATAL: out of memory"))["end"], "crash")
        self.assertEqual(lt.parse(log(*INFO, "FATAL: vkWaitSemaphores: -4 (device lost)"))["end"], "device-lost")
        self.assertEqual(lt.parse(log(*INFO, "window closed"))["end"], "ok")
        self.assertEqual(lt.parse(log(*INFO, "[vk] t=1.0 frame 5 30.0 fps max 40ms"))["end"], "unknown")
        self.assertEqual(lt.parse(log("window closed", "FATAL: late"))["end"], "crash")

    def test_the_label_follows_the_newest_log_not_a_shader_list(self):
        seen = ("s", lt.parse(log("# shader_seen game 1", rec(1))))
        crash = ("c", lt.parse(log("FATAL: x")))
        self.assertEqual(lt.newest_end([seen, crash]), "crash")
        self.assertIsNone(lt.newest_end([seen]))

    def test_an_excerpt_cannot_break_out_of_its_code_span(self):
        evil = "FATAL: `` ``` [x](https://evil.example) @everyone C:\\Users\\someone\\secret\\file.c\x07 " + "y" * 500
        s = lt.summarise(lt.parse(log(evil)))
        line = next(x for x in s.splitlines() if "How the run ended" in x)
        self.assertEqual(line.count("`"), 2, line)
        self.assertNotIn("someone", line)
        self.assertNotIn("\x07", line)
        self.assertLess(len(line), 400)

    def test_the_comment_names_no_other_project(self):
        r = lt.parse(log(*INFO, rec(1), "FATAL: x", "[crash] Your graphics card stopped responding: x"))
        self.assertNotIn("LAS", lt.render([("game.log", r)], 1))
        self.assertIn("kirby-forgotten-land-recompiled.log", lt.render([("game.log", r)], 1))


if __name__ == "__main__":
    unittest.main(verbosity=1)
