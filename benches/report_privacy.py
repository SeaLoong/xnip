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


def fixture_session(prefix):
    SESSION_ROOT.mkdir(parents=True, exist_ok=True)
    return tempfile.TemporaryDirectory(prefix=prefix, dir=SESSION_ROOT)


def public_report(report, homes=None):
    candidates = homes if homes is not None else [str(Path.home()), os.environ.get("USERPROFILE"), os.environ.get("HOME")]
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
            raise ValueError("machine HOME path in protected fixture/tool input; relocate the session instead of changing payload text")
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
        "scope":"Machine HOME prefixes are redacted only in public display metadata after all measurements. Session workspaces are under repository target/native-bench/sessions. Fixture bytes, serialized edit payloads and tool inputs are unchanged; a HOME match there fails closed.",
        "cost_scope":"All cost numbers use the original pre-redaction captured strings, including instruction/DTO layers. Redacted display strings must not be recounted as the originals.",
        "original_values_retained":False, "entries":entries,
    }
    return result
