"""Run the installed Codex apply_patch executor without starting an agent/model.

The argv0 dispatch is part of the real Codex executable, rather than xnip's
patch importer or a Python reproduction of the patch algorithm. Patches use
the executor's stdin path, avoiding the Windows command-line size limit. The
fixture runner supplies the patch and independently checks resulting bytes.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import time

DISPATCH = "--codex-run-as-apply-patch"


def _candidate(executable: str | Path | None) -> Path | None:
    explicit = executable or os.environ.get("XNIP_BENCH_CODEX")
    if explicit:
        return Path(explicit).expanduser().resolve()
    on_path = shutil.which("codex.exe" if os.name == "nt" else "codex")
    if on_path:
        return Path(on_path).resolve()
    if os.name == "nt" and os.environ.get("LOCALAPPDATA"):
        bundled = Path(os.environ["LOCALAPPDATA"]) / "OpenAI" / "Codex" / "bin"
        matches = list(bundled.glob("*/codex.exe"))
        if matches:
            return max(matches, key=lambda path: path.stat().st_mtime).resolve()
    return None


def discover(executable: str | Path | None = None) -> dict:
    """Return auditable availability; only execute --version and empty dispatch."""
    binary = _candidate(executable)
    metadata = {
        "name": "codex-apply-patch",
        "status": "unavailable",
        "execution_level": "native_tool_executor",
        "executable": str(binary) if binary else None,
        "version": None,
        "executable_sha256": None,
        "dispatch": "argv0=apply_patch",
        "transport": "stdin",
        "hidden_dispatch": DISPATCH,
        "model_calls": 0,
        "limitation": "Measures executor calls, not model reasoning, provider billing, approval policy or the full agent session.",
    }
    if binary is None or not binary.is_file():
        metadata["reason"] = "Codex executable not found; pass --codex or set XNIP_BENCH_CODEX."
        return metadata
    try:
        version = subprocess.run([str(binary), "--version"], capture_output=True, timeout=15)
        probe = subprocess.run(["apply_patch"], executable=str(binary), input=b"", capture_output=True, timeout=15)
        metadata["version"] = version.stdout.decode("utf-8", errors="replace").strip()
        metadata["version_stderr"] = version.stderr.decode("utf-8", errors="replace")
        metadata["probe_stdout"] = probe.stdout.decode("utf-8", errors="replace")
        metadata["probe_stderr"] = probe.stderr.decode("utf-8", errors="replace")
        # No-patch dispatch is intentionally read-only and must reject the call.
        if version.returncode or probe.returncode != 2 or "Usage: apply_patch" not in metadata["probe_stderr"]:
            metadata["reason"] = "Installed executable did not expose the expected native apply_patch stdin dispatch."
            return metadata
        digest = hashlib.sha256()
        with binary.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        metadata["executable_sha256"] = digest.hexdigest()
        metadata["status"] = "available"
    except (OSError, subprocess.TimeoutExpired) as error:
        metadata["reason"] = str(error)
    return metadata


def apply_patch(cwd: str | Path, patch: str, executable: str | Path | None = None,
                timeout: float = 15) -> dict:
    """Pass an unchanged patch to Codex; preserve native diagnostics and timing.

    The native argv0 alias reads UTF-8 stdin, including large patches. Setting
    subprocess executable invokes the existing Codex binary directly without a
    shell, symlink, copied binary, config change, or model invocation. Transport
    errors remain separate from native patch rejection.
    """
    binary = _candidate(executable)
    if binary is None:
        raise FileNotFoundError("Codex executable not found")
    command = ["apply_patch"]
    started = time.perf_counter()
    result = {
        "execution_level": "native_tool_executor",
        "transport": "stdin",
        "returncode": None,
        "stdout": "",
        "stderr": "",
        "elapsed_ms": None,
        "transport_error": None,
        "input_utf8_bytes": len(patch.encode("utf-8")),
        "model_calls": 0,
    }
    try:
        completed = subprocess.run(command, executable=str(binary), input=patch.encode("utf-8"),
                                   cwd=Path(cwd), capture_output=True, timeout=timeout)
        result.update({
            "returncode": completed.returncode,
            "stdout": completed.stdout.decode("utf-8", errors="replace"),
            "stderr": completed.stderr.decode("utf-8", errors="replace"),
        })
    except subprocess.TimeoutExpired as error:
        result.update({
            "stdout": (error.stdout or b"").decode("utf-8", errors="replace"),
            "stderr": (error.stderr or b"").decode("utf-8", errors="replace"),
            "transport_error": f"timeout after {timeout}s",
        })
    except OSError as error:
        result["transport_error"] = str(error)
    result["elapsed_ms"] = (time.perf_counter() - started) * 1000
    return result
