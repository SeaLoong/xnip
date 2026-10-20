"""Public benchmark disclosure: relocate sessions, redact HOME display metadata.

Measurement happens first. Fixture bytes, edit programs and actual tool-input
objects are protected: a HOME match there fails instead of changing the payload.
"""
from __future__ import annotations
import hashlib
import os
from pathlib import Path
import re
import tempfile

SESSION_ROOT = Path(__file__).resolve().parents[1]/"target/native-bench/sessions"


def home_values(homes=None):
    return list(homes) if homes is not None else [
        str(Path.home()), os.environ.get("USERPROFILE"), os.environ.get("HOME")]


def system_temp_roots():
    roots = []
    try:
        roots.append(Path(tempfile.gettempdir()))
    except OSError:
        pass
    if os.name == "posix":
        roots.append(Path("/tmp"))
    elif os.name == "nt":
        windows = os.environ.get("SystemRoot") or os.environ.get("WINDIR")
        if windows:
            roots.append(Path(windows)/"Temp")
    return roots


def fixture_session(prefix, *, homes=None, preferred_root=None, temporary_roots=None, configured_root=None):
    """Create a real HOME-external workspace, or fail without rewriting inputs.

    Explicit roots/homes are contract-test seams. Normal callers only supply a
    prefix; XNIP_BENCH_SESSION_ROOT can select a writable external root.
    """
    resolved_homes = []
    try:
        values = home_values(homes)
        for value in values:
            if value:
                resolved_homes.append(Path(value).expanduser().resolve())
    except (OSError, ValueError, RuntimeError):
        raise ValueError("cannot resolve HOME/USERPROFILE for benchmark session safety") from None

    spellings = [str(value).rstrip("\\/").replace("\\", "/").casefold()
                 for value in [*values, *resolved_homes] if value]
    def outside_home(directory):
        visible = str(directory).replace("\\", "/").casefold()
        return (not any(directory.is_relative_to(home) for home in resolved_homes)
                and not any(value in visible for value in spellings if len(value) >= 4))

    attempted = set()
    def create(directory):
        session = None
        try:
            directory = Path(directory).expanduser().resolve()
            if directory in attempted or not outside_home(directory):
                return None
            attempted.add(directory)
            directory.mkdir(parents=True, exist_ok=True, mode=0o700)
            directory = directory.resolve()
            if not outside_home(directory):
                return None
            session = tempfile.TemporaryDirectory(prefix=prefix, dir=directory)
            if outside_home(Path(session.name).resolve()):
                return session
        except (OSError, ValueError, RuntimeError):
            pass
        if session is not None:
            session.cleanup()
        return None

    configured = os.environ.get("XNIP_BENCH_SESSION_ROOT") if configured_root is None else configured_root
    if configured:
        session = create(configured)
        if session is None:
            raise ValueError("configured XNIP_BENCH_SESSION_ROOT is not writable outside HOME/USERPROFILE")
        return session
    roots = [SESSION_ROOT if preferred_root is None else preferred_root]
    roots.extend(system_temp_roots() if temporary_roots is None else temporary_roots)
    for directory in roots:
        session = create(directory)
        if session is not None:
            return session
    raise ValueError("no writable benchmark session directory outside HOME/USERPROFILE; configure XNIP_BENCH_SESSION_ROOT")


def public_report(report, homes=None):
    candidates = home_values(homes)
    variants = set()
    for home in candidates:
        if not home:
            continue
        plain = str(home).rstrip("\\/")
        if len(plain) < 4:
            continue
        forward = plain.replace("\\", "/")
        backward = plain.replace("/", "\\")
        variants.update((plain, forward))
        for width in (1,2,4,8):
            variants.add(backward.replace("\\", "\\"*width))
    pattern = re.compile("|".join(re.escape(value) for value in sorted(variants, key=lambda value: (-len(value), value))),
                         re.IGNORECASE) if variants else None
    entries = []
    cache = {}

    def redact(value, pointer, protected=False):
        if pattern is None or pattern.search(value) is None:
            return value
        if protected:
            raise ValueError("machine HOME path in protected fixture/tool input at "
                + (pointer or "/") + "; relocate the session instead of changing payload text")
        if value not in cache:
            display = pattern.sub("<HOME>", value)
            cache[value] = (display, hashlib.sha256(value.encode("utf8")).hexdigest(),
                            hashlib.sha256(display.encode("utf8")).hexdigest(), len(value.encode("utf8")))
        display, original_sha, display_sha, original_size = cache[value]
        entries.append({"json_pointer":pointer, "original_sha256":original_sha,
            "display_sha256":display_sha, "original_utf8_bytes":original_size})
        return display

    def pointer(parts):
        return "/"+"/".join(str(part).replace("~","~0").replace("/","~1") for part in parts) if parts else ""

    def walk(value, parts=(), protected=False):
        if isinstance(value, str):
            return redact(value, pointer(parts), protected)
        if isinstance(value, list):
            return [walk(item, (*parts,index), protected) for index,item in enumerate(value)]
        if isinstance(value, dict):
            result = {}
            for key,item in value.items():
                fixture_field = (len(parts) == 2 and parts[0] in {"cases","probes","scenarios"}
                                 and key in {"files","expected","edits","edit"})
                protect = protected or fixture_field or key in {"input","arguments","native_input","payload","utf8"}
                # Never put a private original dictionary key in the disclosure log.
                display_key = pattern.sub("<HOME>", key) if pattern is not None else key
                if display_key != key:
                    redact(key, pointer((*parts,display_key)), protect)
                if display_key in result:
                    raise ValueError("HOME display redaction would collide with another metadata key")
                result[display_key] = walk(item, (*parts,display_key), protect)
            return result
        return value

    result = walk(report)
    result["disclosure_redaction"] = {
        "schema_version":1, "replacement":"<HOME>", "changed_strings":len(entries),
        "scope":"Machine HOME prefixes are redacted only in public display metadata after all measurements. Session workspaces resolve outside every HOME/USERPROFILE root: repository target/native-bench/sessions when safe, otherwise an external temporary directory; XNIP_BENCH_SESSION_ROOT must satisfy the same checks. Fixture bytes, serialized edit payloads and tool inputs are unchanged; a HOME match there fails closed.",
        "cost_scope":"All cost numbers use the original pre-redaction captured strings, including instruction/DTO layers. Redacted display strings must not be recounted as the originals.",
        "original_values_retained":False, "entries":entries,
    }
    return result
