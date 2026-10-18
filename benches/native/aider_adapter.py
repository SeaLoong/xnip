#!/usr/bin/env python3
"""Offline worker using pinned, unmodified aider get_edits/apply_edits and IO.

Run with the isolated aider virtualenv. stdin/stdout are JSON Lines:
  {"action":"describe"}
  {"action":"execute","format":"diff","root":"...","chat_files":["a.txt"],
   "response":"a.txt\\n```\\n<<<<<<< SEARCH\\n...", "line_endings":"lf"}

This is an editing-kernel experiment, not an agent/model benchmark. It skips
Coder initialization, model calls, edit authorization, git commits, linting,
shell commands, and reflection. Parsing, matching, read/write, errors, and edit
format prompts come directly from the upstream package. It neither simulates
matching nor replaces native InputOutput read/write. model_visible_result is
null because get_edits/apply_edits do not assemble the complete model message.
"""
from __future__ import annotations

import contextlib
import hashlib
import importlib.metadata
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import time

PINNED_COMMIT = "5dc9490bb35f9729ef2c95d00a19ccd30c26339c"
UPSTREAM = "https://github.com/Aider-AI/aider"
SOURCE_FILES = (
    "aider/coders/base_coder.py", "aider/coders/editblock_coder.py",
    "aider/coders/wholefile_coder.py", "aider/coders/udiff_coder.py",
    "aider/coders/search_replace.py", "aider/io.py",
    "aider/coders/base_prompts.py", "aider/coders/editblock_prompts.py",
    "aider/coders/wholefile_prompts.py", "aider/coders/udiff_prompts.py",
)


def load_native():
    """Import the installed source without constructing a model or full Coder."""
    start = time.perf_counter()
    from aider import __version__
    from aider.coders.editblock_coder import EditBlockCoder
    from aider.coders.wholefile_coder import WholeFileCoder
    from aider.coders.udiff_coder import UnifiedDiffCoder
    from aider.io import InputOutput
    import aider

    source = Path(aider.__file__).resolve().parents[1]
    revision = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True,
        stderr=subprocess.PIPE,
    ).strip()
    if revision != PINNED_COMMIT:
        raise RuntimeError(f"aider source revision {revision} != pin {PINNED_COMMIT}")
    changed = subprocess.check_output(
        ["git", "-C", str(source), "diff", "HEAD", "--", *SOURCE_FILES],
        stderr=subprocess.PIPE,
    )
    if changed:
        raise RuntimeError("aider editing source differs from the pinned official revision")
    hashes = {name: hashlib.sha256((source / name).read_bytes()).hexdigest()
              for name in SOURCE_FILES}
    return {
        "classes": {"diff": EditBlockCoder, "whole": WholeFileCoder,
                    "udiff": UnifiedDiffCoder},
        "io_class": InputOutput,
        "metadata": {
            "status": "available", "harness": "aider", "version": __version__,
            "distribution_version": importlib.metadata.version("aider-chat"),
            "upstream": UPSTREAM, "revision": revision,
            "source_url": f"{UPSTREAM}/tree/{revision}",
            "source_sha256": hashes,
            "source_hash": hashlib.sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
            "dependencies": {name: importlib.metadata.version(name)
                             for name in ("diff-match-patch", "tqdm", "GitPython")},
            "execution_level": "native-editing-kernel",
            "native_methods": ["get_edits", "apply_edits", "InputOutput.read_text",
                               "InputOutput.write_text"],
            "model_calls": 0, "native_tool_schemas": None,
            "line_endings_default": "platform",
            "line_endings_options": ["platform", "lf", "crlf"],
            "import_ms": (time.perf_counter() - start) * 1000,
        },
    }


def describe(native):
    formats = {}
    for name, cls in native["classes"].items():
        prompts = cls.gpt_prompts
        templates = {}
        for key in dir(prompts):
            if key.startswith("_"):
                continue
            value = getattr(prompts, key)
            if isinstance(value, (str, list, dict)):
                templates[key] = value
        formats[name] = {
            "class": f"{cls.__module__}.{cls.__name__}",
            "prompt_templates": templates,
            "prompt_scope": "raw native format templates; not a rendered full agent prompt",
        }
    return {**native["metadata"], "formats": formats,
            "model_visible_result": None}


def execute(native, request):
    root = Path(request["root"]).resolve(strict=True)
    if not root.is_dir():
        raise ValueError("root must be an existing isolated fixture directory")
    cls = native["classes"][request["format"]]
    response = request["response"]
    if not isinstance(response, str):
        raise ValueError("response must be the unmodified model-format text")
    chat_files = request.get("chat_files", [])
    # Coder's model/repo orchestration is deliberately not constructed. These
    # fields are exactly what the real parser and edit application methods use.
    coder = object.__new__(cls)
    coder.root = str(root)
    coder.abs_root_path_cache = {}
    coder.abs_fnames = {str((root / name).resolve()) for name in chat_files}
    coder.abs_read_only_fnames = set()
    coder.partial_response_content = response
    coder.multi_response_content = ""
    coder.fence = ("`" * 3, "`" * 3)
    coder.shell_commands = []
    output, diagnostics, transcript = io.StringIO(), io.StringIO(), io.StringIO()
    setup_output, setup_diagnostics = io.StringIO(), io.StringIO()
    setup_started = time.perf_counter()
    with contextlib.redirect_stdout(setup_output), contextlib.redirect_stderr(setup_diagnostics):
        coder.io = native["io_class"](
            pretty=False, yes=True, fancy_input=False, output=transcript,
            input_history_file=None, chat_history_file=None, llm_history_file=None,
            encoding="utf-8", root=str(root),
            line_endings=request.get("line_endings", "platform"),
        )
    setup_elapsed = (time.perf_counter() - setup_started) * 1000
    parsed = None
    returned = None
    error = None
    stage = "get_edits"
    started = time.perf_counter()
    with contextlib.redirect_stdout(output), contextlib.redirect_stderr(diagnostics):
        try:
            parsed = coder.get_edits()
            stage = "apply_edits"
            returned = coder.apply_edits(parsed)
            stage = "complete"
        except Exception as exc:
            error = {"type": type(exc).__name__, "message": str(exc), "stage": stage}
    elapsed = (time.perf_counter() - started) * 1000
    return {
        "ok": error is None, "format": request["format"],
        "native_payload": response, "parsed_edits": parsed,
        "native_return": returned, "native_error": error,
        "stdout": output.getvalue(), "stderr": diagnostics.getvalue(),
        "io_output": transcript.getvalue(), "shell_commands": coder.shell_commands,
        "model_visible_result": None, "edit_elapsed_ms": elapsed,
        "setup_elapsed_ms": setup_elapsed, "setup_stdout": setup_output.getvalue(),
        "setup_stderr": setup_diagnostics.getvalue(),
        "line_endings": request.get("line_endings", "platform"),
    }


def main():
    if hasattr(sys.stdin, "reconfigure"):
        sys.stdin.reconfigure(encoding="utf-8")
        sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    # LiteLLM is lazy upstream; these also prohibit remote metadata refresh if a
    # future imported dependency eagerly loads it. No API/model is constructed.
    os.environ["LITELLM_LOCAL_MODEL_COST_MAP"] = "True"
    os.environ["DO_NOT_TRACK"] = "1"

    def no_network(event, args):
        if event in {"socket.connect", "socket.getaddrinfo"}:
            raise RuntimeError("network disabled in native editing benchmark")

    sys.addaudithook(no_network)
    bootstrap_out, bootstrap_err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(bootstrap_out), contextlib.redirect_stderr(bootstrap_err):
        native = load_native()
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
            action = request.get("action", "execute")
            if action == "describe":
                result = describe(native)
                result["bootstrap_stdout"] = bootstrap_out.getvalue()
                result["bootstrap_stderr"] = bootstrap_err.getvalue()
            elif action == "execute":
                result = execute(native, request)
            else:
                raise ValueError(f"unsupported action: {action}")
        except Exception as exc:
            result = {"ok": False, "adapter_error": {"type": type(exc).__name__,
                      "message": str(exc)}, "model_visible_result": None}
        print(json.dumps(result, ensure_ascii=False, separators=(",", ":")), flush=True)


if __name__ == "__main__":
    main()
