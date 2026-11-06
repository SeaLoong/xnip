"""Actual pinned Aider Coder construction and reply dispatch, without generation."""
from contextlib import ExitStack, redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
from unittest.mock import patch

os.environ["LITELLM_LOCAL_MODEL_COST_MAP"] = "True"
os.environ["DO_NOT_TRACK"] = "1"
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/aider-native"))
sys.path.insert(0, str(ROOT / "integrations/native-python"))

from aider.analytics import Analytics
from aider.coders import Coder
from aider.io import InputOutput
from aider.models import Model, register_litellm_models
from aider.llm import litellm
from xnip_aider import create_xnip_coder

BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT / "target/debug/xnip")).resolve()


class RecordingIO(InputOutput):
    def __init__(self, **kwargs):
        self.errors, self.confirmations = [], []
        self.transcript = io.StringIO()
        super().__init__(pretty=False, fancy_input=False, output=self.transcript,
                         input_history_file=None, chat_history_file=None, llm_history_file=None,
                         **kwargs)

    def tool_error(self, *args, **kwargs):
        self.errors.append(" ".join(str(value) for value in args))
        return super().tool_error(*args, **kwargs)

    def confirm_ask(self, question, **kwargs):
        answer = super().confirm_ask(question, **kwargs)
        self.confirmations.append({"question": question, "subject": kwargs.get("subject"),
                                   "confirmed": answer})
        return answer


class LocalAnalytics(Analytics):
    def __init__(self, path):
        self.path = path
        super().__init__(permanently_disable=True)

    def get_data_file_path(self):
        return self.path


class AiderFixture:
    def __init__(self, cwd, *, native=True, edit_format="diff", files=(), yes=True,
                 line_endings="lf", encoding="utf-8", dry_run=False, use_git=False,
                 read_only_fnames=None, create_client=None, writable_roots=None,
                 executable=BINARY):
        self.cwd = Path(cwd)
        self.cwd.mkdir(parents=True, exist_ok=True)
        model_metadata = self.cwd / ".aider-test-model.json"
        model_metadata.write_text(json.dumps({"gpt-4o": {"max_input_tokens": 128000,
            "max_output_tokens": 4096, "litellm_provider": "openai", "mode": "chat",
            "input_cost_per_token": 0, "output_cost_per_token": 0}}))
        register_litellm_models([str(model_metadata)])
        self.model = Model("gpt-4o", weak_model="gpt-4o", editor_model="gpt-4o")
        self.bootstrap = io.StringIO()
        with redirect_stdout(self.bootstrap), redirect_stderr(self.bootstrap):
            self.io = RecordingIO(root=str(self.cwd), yes=yes, line_endings=line_endings,
                                  encoding=encoding, dry_run=dry_run)
        self.analytics = LocalAnalytics(self.cwd / ".aider-test-analytics.json")
        names = [str(self.cwd / name) for name in files]
        if not names:
            anchor = self.cwd / ".aider-test-anchor"
            anchor.write_text("Aider SDK fixture\n")
            names = [str(anchor)]
        options = dict(main_model=self.model, edit_format=edit_format, fnames=names,
                       io=self.io, analytics=self.analytics, use_git=use_git,
                       map_tokens=0, auto_commits=False, dirty_commits=False,
                       auto_lint=False, auto_test=False, dry_run=dry_run,
                       read_only_fnames=read_only_fnames)
        self.native = native
        with redirect_stdout(self.bootstrap), redirect_stderr(self.bootstrap):
            self.session = create_xnip_coder(executable=executable, writable_roots=writable_roots,
                **({"create_client": create_client} if create_client else {}), **options) if native else None
            self.coder = self.session.coder if native else Coder.create(**options)
        self.guard = ExitStack()
        self.model_calls = self.guard.enter_context(patch.object(Model, "send_completion",
            side_effect=AssertionError("Aider tool dispatch reached model generation")))
        self.litellm_calls = self.guard.enter_context(patch.object(litellm, "completion",
            side_effect=AssertionError("Aider tool dispatch reached LiteLLM generation")))

    @property
    def model_call_count(self):
        return self.model_calls.call_count + self.litellm_calls.call_count

    def apply(self, response):
        before_errors = len(self.io.errors)
        output = io.StringIO()
        with redirect_stdout(output), redirect_stderr(output):
            if self.session:
                edited = self.session.apply_response(response)
            else:
                self.coder.partial_response_content = response
                self.coder.multi_response_content = ""
                self.coder.reflected_message = ""
                edited = self.coder.apply_updates()
        assert self.model_call_count == 0
        return {"edited": sorted(edited), "reflection": self.coder.reflected_message,
                "errors": self.io.errors[before_errors:], "diagnostics": output.getvalue()}

    def close(self):
        if self.session:
            self.session.close()
        assert self.model_call_count == 0
        self.guard.close()


def diff_response(path, old, new):
    fence = "`" * 3
    return f"{path}\n{fence}\n<<<<<<< SEARCH\n{old}{'' if old.endswith(chr(10)) or not old else chr(10)}=======\n{new}{'' if new.endswith(chr(10)) or not new else chr(10)}>>>>>>> REPLACE\n{fence}\n"
