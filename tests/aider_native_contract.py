"""Real Aider scripting/native IO contracts; model generation remains zero."""
import difflib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from aider_native_runtime import AiderFixture, BINARY, ROOT, diff_response
from aider.coders import __all__ as coder_classes
from xnip_aider import SUPPORTED_FORMATS, create_xnip_coder
from xnip_native_client import NativeClient

sys.path.insert(0, str(ROOT / "benches"))
from edit_cost import scenarios


def lf(text):
    return text.replace("\r\n", "\n").replace("\r", "\n")


def response_for(runtime, files, expected, edit_format="diff"):
    bodies = []
    fence = "`" * 3
    for name, before in files.items():
        path = runtime.coder.get_rel_fname(str(runtime.cwd / name))
        old, new = lf(before), lf(expected[name])
        if edit_format in ("whole", "editor-whole"):
            bodies.append(f"{path}\n{fence}\n{new}{'' if new.endswith(chr(10)) else chr(10)}{fence}\n")
        elif edit_format in ("udiff", "udiff-simple"):
            before_lines = [line + "\n" for line in old.splitlines()]
            after_lines = [line + "\n" for line in new.splitlines()]
            delta = "".join(difflib.unified_diff(before_lines, after_lines, fromfile=path, tofile=path, n=3))
            bodies.append(f"{fence}diff\n{delta}{fence}\n")
        else:
            bodies.append(diff_response(path, old, new))
    return "\n".join(bodies)


class AiderNativeContract(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="aider-native-", dir=ROOT / "target")
        self.cwd = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)

    def fixture(self, **kwargs):
        runtime = AiderFixture(self.cwd, **kwargs)
        self.addCleanup(runtime.close)
        return runtime

    def write(self, name, content):
        path = self.cwd / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content if isinstance(content, bytes) else content.encode())
        return path

    def test_real_registry_prompts_and_model_counter(self):
        self.write("a.txt", "old\n")
        native = self.fixture(files=["a.txt"])
        original = self.fixture(native=False, files=["a.txt"])
        self.assertIs(type(native.coder), type(original.coder))
        self.assertEqual(native.coder.gpt_prompts.__dict__, original.coder.gpt_prompts.__dict__)
        self.assertIs(native.coder.gpt_prompts, original.coder.gpt_prompts)
        self.assertTrue(set(SUPPORTED_FORMATS).issubset({cls.edit_format for cls in coder_classes}))
        result = native.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertEqual(result["errors"], [])
        self.assertEqual((self.cwd / "a.txt").read_bytes(), b"new\n")
        self.assertEqual(native.model_call_count, 0)

    def test_all_eight_io_backed_format_routes(self):
        for edit_format in SUPPORTED_FORMATS:
            with self.subTest(edit_format=edit_format):
                self.write("a.txt", "old\n")
                runtime = self.fixture(edit_format=edit_format, files=["a.txt"])
                result = runtime.apply(response_for(runtime, {"a.txt": "old\n"}, {"a.txt": "new\n"}, edit_format))
                self.assertEqual(result["errors"], [], result)
                self.assertEqual((self.cwd / "a.txt").read_bytes(), b"new\n")
                self.assertEqual(runtime.model_call_count, 0)

    def test_original_authorization_and_new_file_creation(self):
        self.write("chat.txt", "chat\n")
        self.write("other.txt", "old\n")
        denied = self.fixture(files=["chat.txt"], yes=False)
        result = denied.apply(diff_response("other.txt", "old\n", "new\n"))
        self.assertEqual(result["errors"], [])
        self.assertEqual((self.cwd / "other.txt").read_text(), "old\n")
        self.assertTrue(any(not event["confirmed"] for event in denied.io.confirmations))
        allowed = self.fixture(files=["chat.txt"], yes=True)
        result = allowed.apply(diff_response("new.txt", "", "created\n"))
        self.assertEqual(result["errors"], [], result)
        self.assertEqual((self.cwd / "new.txt").read_text(), "created\n")
        self.assertTrue(any(event["question"].startswith("Create new file") for event in allowed.io.confirmations))

    def test_sdk_dry_run_and_core_authority(self):
        self.write("a.txt", "old\n")
        preview = self.fixture(files=["a.txt"], dry_run=True)
        preview.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertEqual((self.cwd / "a.txt").read_text(), "old\n")
        self.assertIsNone(preview.session.io._client)
        denied = self.fixture(files=["a.txt"], writable_roots=[])
        result = denied.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertTrue(result["errors"])
        self.assertEqual((self.cwd / "a.txt").read_text(), "old\n")

    def test_encoding_bom_and_line_endings_follow_original_io(self):
        for encoding, line_endings, raw in [
            ("utf-8", "lf", b"\xef\xbb\xbfhead\r\nold\r\nend\r\n"),
            ("utf-8", "crlf", "零\nold\nend\n".encode()),
            ("latin-1", "lf", b"caf\xe9\nold\n"),
        ]:
            results = []
            for native in [False, True]:
                self.write("a.txt", raw)
                runtime = self.fixture(native=native, files=["a.txt"], encoding=encoding, line_endings=line_endings)
                result = runtime.apply(diff_response("a.txt", "old\n", "new\n"))
                self.assertEqual(result["errors"], [], result)
                results.append(((self.cwd / "a.txt").read_bytes(), result))
            self.assertEqual(*results)
        # TextIO opens/truncates before encoding its write; the core adapter
        # prepares its byte payload first and retains storage on encoding error.
        stored = []
        for native in [False, True]:
            self.write("a.txt", b"old\n")
            runtime = self.fixture(native=native, files=["a.txt"], encoding="latin-1")
            result = runtime.apply(diff_response("a.txt", "old\n", "\u6c49\u5b57\n"))
            self.assertTrue(result["errors"])
            stored.append((self.cwd / "a.txt").read_bytes())
        self.assertEqual(stored, [b"", b"old\n"])

    def test_restoration_keeps_coder_and_closes_subprocess(self):
        self.write("a.txt", "old\n")
        runtime = self.fixture(files=["a.txt"])
        prompts = runtime.coder.gpt_prompts
        runtime.apply(diff_response("a.txt", "old\n", "native\n"))
        process = runtime.session.io.client.process
        runtime.session.close()
        self.assertIsNotNone(process.poll())
        self.assertIs(runtime.coder.gpt_prompts, prompts)
        result = runtime.apply(diff_response("a.txt", "native\n", "restored\n"))
        self.assertEqual(result["errors"], [], result)
        self.assertEqual((self.cwd / "a.txt").read_text(), "restored\n")

    def test_external_change_after_matching_preserves_external_bytes(self):
        self.write("a.txt", "old\n")
        def client(binary, cwd):
            editor = NativeClient(binary, cwd)
            request = editor.request
            def changed(action, **fields):
                if action == "plan":
                    (self.cwd / "a.txt").write_bytes(b"external\r\n")
                return request(action, **fields)
            editor.request = changed
            return editor
        runtime = self.fixture(files=["a.txt"], create_client=client)
        result = runtime.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertTrue(result["errors"])
        self.assertEqual((self.cwd / "a.txt").read_bytes(), b"external\r\n")

    def test_cancel_after_plan_preserves_storage(self):
        self.write("a.txt", "old\n")
        holder = {}
        def client(binary, cwd):
            editor = NativeClient(binary, cwd)
            request = editor.request
            def cancelled(action, **fields):
                result = request(action, **fields)
                if action == "plan": holder["session"].cancel()
                return result
            editor.request = cancelled
            return editor
        runtime = self.fixture(files=["a.txt"], create_client=client)
        holder["session"] = runtime.session
        result = runtime.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertTrue(result["errors"])
        self.assertEqual((self.cwd / "a.txt").read_bytes(), b"old\n")
        self.assertGreater(runtime.session.io.client.actions["abort"], 0)

    def test_actual_git_repo_policy_keeps_history_under_host_control(self):
        self.write("a.txt", "old\n")
        def git(*args):
            return subprocess.run(["git", "-C", str(self.cwd), *args], check=True, capture_output=True, text=True).stdout
        git("init", "--quiet")
        git("config", "user.name", "Aider tool fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("add", "a.txt")
        git("commit", "--quiet", "-m", "baseline")
        before = git("rev-parse", "HEAD")
        runtime = self.fixture(files=["a.txt"], use_git=True)
        self.assertIsNotNone(runtime.coder.repo)
        runtime.apply(diff_response("a.txt", "old\n", "new\n"))
        self.assertEqual(git("rev-parse", "HEAD"), before)
        self.assertIn("+new", git("diff", "--", "a.txt"))
        self.assertEqual(runtime.model_call_count, 0)

    def test_real_installed_python_packages_dispatch_a_coder(self):
        stage = self.cwd / "staged"
        stage.mkdir()
        projects = []
        for name in ["native-python", "aider-native"]:
            source, target = ROOT / "integrations" / name, stage / name
            target.mkdir()
            for file in source.iterdir():
                if file.is_file(): shutil.copy2(file, target / file.name)
            projects.append(str(target))
        installed = self.cwd / "installed"
        subprocess.run([sys.executable, "-m", "pip", "install", "--quiet", "--disable-pip-version-check",
            "--no-deps", "--no-build-isolation", "--target", str(installed), *projects], check=True,
            capture_output=True, text=True)
        code = r'''
import json,sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import xnip_aider,xnip_native_client
assert Path(xnip_aider.__file__).resolve().parent==Path(sys.argv[1]).resolve()
assert Path(xnip_native_client.__file__).resolve().parent==Path(sys.argv[1]).resolve()
sys.path.insert(0,sys.argv[2])
from aider_native_runtime import AiderFixture,diff_response
cwd=Path(sys.argv[3]);(cwd/'a.txt').write_text('old\n')
r=AiderFixture(cwd,files=['a.txt'])
try:
 result=r.apply(diff_response('a.txt','old\n','installed\n'))
 assert not result['errors'] and (cwd/'a.txt').read_text()=='installed\n'
 process=r.session.io.client.process
finally:r.close()
assert process.poll() is not None and r.model_call_count==0
print(json.dumps({'installed':True,'model_calls':0}))
'''
        work = self.cwd / "package-work"
        work.mkdir()
        result = subprocess.run([sys.executable, "-c", code, str(installed), str(ROOT / "tests"), str(work)],
            check=True, capture_output=True, text=True, env={**os.environ, "XNIP_TEST_BINARY": str(BINARY)})
        self.assertEqual(json.loads(result.stdout.splitlines()[-1]), {"installed": True, "model_calls": 0})

    def test_each_reply_gets_a_fresh_operation_snapshot(self):
        self.write("a.txt", "old\n")
        runtime = self.fixture(files=["a.txt"])
        runtime.apply(diff_response("a.txt", "old\n", "first\n"))
        self.write("a.txt", "external\n")
        result = runtime.apply(diff_response("a.txt", "external\n", "second\n"))
        self.assertEqual(result["errors"], [], result)
        self.assertEqual((self.cwd / "a.txt").read_text(), "second\n")

    def shared_fixture(self, case):
        for edit_format in ["diff", "whole", "udiff"]:
            outcomes = []
            for native in [False, True]:
                for name, text in case.files.items(): self.write(name, text)
                runtime = self.fixture(native=native, edit_format=edit_format, files=case.files)
                result = runtime.apply(response_for(runtime, case.files, case.expected(), edit_format))
                self.assertEqual(result["errors"], [], (case.name, edit_format, native, result))
                stored = {name: (self.cwd / name).read_bytes() for name in case.files}
                expected = {name: (lf(text) + ("" if lf(text).endswith("\n") else "\n")).encode()
                            for name, text in case.expected().items()}
                self.assertEqual(stored, expected, (case.name, edit_format, native))
                outcomes.append((stored, result))
            self.assertEqual(*outcomes)


for case in scenarios():
    setattr(AiderNativeContract, "test_bytes_" + case.name.replace("-", "_"),
            lambda self, fixture=case: self.shared_fixture(fixture))


if __name__ == "__main__":
    unittest.main()
