"""Offline CLI planning latency/working-set probes; does not install or contact a network."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tempfile
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    paths += list((ROOT / "src").rglob("*.rs"))
    paths += [p for p in (ROOT / "benches").rglob("*") if p.is_file()
              and "__pycache__" not in p.parts]
    return {str(p.relative_to(ROOT)).replace("\\", "/"): digest(p)
            for p in sorted(paths)}


def memory_sampler():
    if os.name != "nt":
        return lambda child: None
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in
            ["peak", "working", "peak_paged", "paged", "peak_nonpaged",
             "nonpaged", "pagefile", "peak_pagefile"]]

    query = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]

    def sample(child):
        counters = Counters()
        counters.cb = ctypes.sizeof(counters)
        return counters.peak if query(int(child._handle), ctypes.byref(counters), counters.cb) else None

    return sample


def invoke(binary, directory, manifest, check, sample):
    args = [str(binary), "apply", str(manifest), "--format", "json", "--quiet"]
    if check:
        args.append("--check")
    started = time.perf_counter()
    child = subprocess.Popen(args, cwd=directory,
                             env=dict(os.environ, XNIP_AUTO_UPDATE="0"),
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    deadline = started + 30
    peak = None
    while child.poll() is None:
        observed = sample(child)
        if observed is not None:
            peak = max(peak or 0, observed)
        if time.perf_counter() > deadline:
            child.kill()
            child.wait()
            raise TimeoutError("planning probe exceeded 30 seconds")
        time.sleep(0.001)
    out, err = child.communicate()
    elapsed = (time.perf_counter() - started) * 1000
    if child.returncode or out or err:
        raise AssertionError((child.returncode, out, err))
    return {"elapsed_ms": elapsed, "peak_working_set_bytes": peak}


def scenario(binary, directory, count, width, edits, kind, repeats, sample):
    if width == 2:
        lines = [b"x\n"] * count
    else:
        lines = [f"{i:08d} ".encode() + b"x" * (width - 10) + b"\n" for i in range(count)]
    before = b"".join(lines)
    selected = {int(count * (i + 1) / (edits + 1)) for i in range(edits)}
    replacements = {}
    for ordinal, index in enumerate(sorted(selected)):
        if kind == "ordered_equal":
            replacements[index] = lines[index].replace(b"x", b"Y")
        elif kind == "ordered_resize":
            replacements[index] = (b"short\n" if ordinal % 2 == 0
                                   else f"expanded {index}: ".encode() + b"y" * 128 + b"\n")
        elif kind == "ordered_noop":
            replacements[index] = lines[index]
        else:
            replacements[index] = b"changed\n"
    expected = b"".join(replacements.get(i, line) for i, line in enumerate(lines))
    operations = []
    for index in sorted(selected):
        if kind == "grouped_lines":
            operations.append({"op": "replace", "file": "input", "lines": index + 1,
                               "was": lines[index].decode(), "text": "changed"})
        else:
            operations.append({"op": "edit", "file": "input",
                               "old": lines[index].decode(), "text": replacements[index].decode()})
    target, manifest = directory / "input", directory / "manifest.json"
    manifest.write_text(json.dumps(operations), encoding="utf8")
    target.write_bytes(before)
    # Check the complete write result independently outside the measured region.
    invoke(binary, directory, manifest, False, sample)
    assert target.read_bytes() == expected
    target.write_bytes(before)
    invoke(binary, directory, manifest, True, sample)
    runs = [invoke(binary, directory, manifest, True, sample) for _ in range(repeats)]
    assert target.read_bytes() == before, "checks changed the original bytes"
    return {"kind": kind, "bytes": len(before), "lines": count, "edits": edits,
            "write_oracle_passed": True, "checks_unchanged": True, "runs": runs,
            "median_ms": statistics.median(run["elapsed_ms"] for run in runs),
            "min_ms": min(run["elapsed_ms"] for run in runs),
            "max_peak_working_set_bytes": max(
                (run["peak_working_set_bytes"] for run in runs
                 if run["peak_working_set_bytes"] is not None), default=None)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--xnip", type=Path, default=ROOT / "target/release" /
                        ("xnip.exe" if os.name == "nt" else "xnip"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    binary = args.xnip.resolve()
    initial_sources, initial_binary = sources(), digest(binary)
    env = dict(os.environ, XNIP_AUTO_UPDATE="0")
    version = subprocess.check_output([str(binary), "--version"], env=env, text=True).strip()
    report = {"schema_version": 1, "created_at_utc": datetime.now(timezone.utc).isoformat(),
              "version": version, "binary": str(binary), "binary_sha256": initial_binary,
              "source_files_sha256": initial_sources,
              "git_commit": subprocess.check_output(
                  ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "platform": platform.platform(), "python": platform.python_version(),
              "cpu": platform.processor() or os.environ.get("PROCESSOR_IDENTIFIER"),
              "logical_cpus": os.cpu_count(), "repeats": args.repeats,
              "mode": "CLI apply --check --quiet; cached local reads; updates disabled",
              "memory_scope": "Windows OS cumulative peak working set sampled every 1ms; null elsewhere",
              "limitations": ["Current binary strategy comparison, not a historical before/after",
                              "Whole-process latency includes startup and manifest/filesystem reads",
                              "Small fixed fixture set; no agent/model/token measurement",
                              "Working-set samples can miss the final interval before process exit"],
              "results": []}
    sample = memory_sampler()
    with tempfile.TemporaryDirectory(prefix="xnip-core-perf-") as temporary:
        directory = Path(temporary)
        for edits in [1, 8, 32, 128]:
            for kind in ["grouped_lines", "ordered_exact"]:
                result = scenario(binary, directory, 65536, 64, edits, kind, args.repeats, sample)
                report["results"].append(result)
                print(f"{kind} 65536 lines / {edits} edits: {result['median_ms']:.3f} ms", flush=True)
        for kind in ["ordered_equal", "ordered_resize", "ordered_noop"]:
            result = scenario(binary, directory, 65536, 64, 128, kind, args.repeats, sample)
            report["results"].append(result)
            print(f"{kind} 65536 lines / 128 edits: {result['median_ms']:.3f} ms", flush=True)
        for edits in [1, 32]:
            result = scenario(binary, directory, 2097152, 2, edits, "grouped_lines", args.repeats, sample)
            report["results"].append(result)
            print(f"grouped_lines 2097152 lines / {edits} edits: {result['median_ms']:.3f} ms", flush=True)
    if initial_sources != sources() or initial_binary != digest(binary):
        raise RuntimeError("source or binary changed during the run; no report written")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")


if __name__ == "__main__":
    main()
