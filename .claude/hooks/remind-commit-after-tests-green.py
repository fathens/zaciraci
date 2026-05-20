#!/usr/bin/env python3
"""PostToolUse hook that nudges the agent to commit after a clean cargo run.

Background
----------
``CONTRIBUTING.md`` requires one logical change per commit, and the project
``CLAUDE.md`` reinforces "commit after each step, don't batch at the end".
The natural commit boundary in this codebase is "tests/lint pass on a piece
of work" — but it is easy to keep editing past that point and accumulate
several logical changes into a single uncommitted diff. Once that happens
the only ways out (``git stash`` + replay, manual hunk staging) are slow
and error-prone.

This hook fires *only* at that natural boundary: after a ``Bash`` tool call
whose command included ``cargo test`` or ``cargo clippy`` exited 0 with a
non-empty working tree. It then injects an ``additionalContext`` reminder
into the agent context so the agent sees the prompt to commit at the
moment it matters, without paying the per-conversation token cost of a
permanent memory entry.

Output contract
---------------
* Always ``exit 0`` — this hook never blocks.
* Emits a single JSON object on stdout when the reminder is warranted, in
  the documented Claude Code shape::

      {"hookSpecificOutput": {"hookEventName": "PostToolUse",
                              "additionalContext": "..."}}

* Silent (no stdout, exit 0) when the trigger conditions are not met.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys


CARGO_GREEN_RE = re.compile(r"\bcargo\s+(?:test|clippy)\b")
MAX_LISTED_ENTRIES = 10


def _load_payload() -> dict:
    try:
        return json.load(sys.stdin)
    except json.JSONDecodeError:
        return {}


def _bash_command(payload: dict) -> str:
    return (payload.get("tool_input") or {}).get("command", "") or ""


def _bash_exit_code(payload: dict) -> int:
    response = payload.get("tool_response") or {}
    code = response.get("exit_code")
    if isinstance(code, int):
        return code
    return -1


def _git_porcelain() -> list[str]:
    try:
        result = subprocess.run(
            ["git", "status", "--porcelain"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return []
    return [line for line in result.stdout.splitlines() if line.strip()]


def _git_shortstat() -> str:
    try:
        result = subprocess.run(
            ["git", "diff", "--shortstat", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout.strip()


def _build_message(porcelain: list[str], shortstat: str) -> str:
    head = (
        "✅ cargo tests/lint green with uncommitted changes"
        f" ({shortstat or 'see git status'}).\n"
        "If this represents one completed logical change, commit it now per "
        "CONTRIBUTING.md (`1 commit = 1 logical change`) before starting the "
        "next change.\n"
        "Pending entries:"
    )
    listed = porcelain[:MAX_LISTED_ENTRIES]
    body = "\n".join(f"  {line}" for line in listed)
    extra = (
        f"\n  ... ({len(porcelain) - MAX_LISTED_ENTRIES} more)"
        if len(porcelain) > MAX_LISTED_ENTRIES
        else ""
    )
    return f"{head}\n{body}{extra}"


def main() -> int:
    payload = _load_payload()
    command = _bash_command(payload)
    if not command:
        return 0

    if not CARGO_GREEN_RE.search(command):
        return 0

    if _bash_exit_code(payload) != 0:
        return 0

    porcelain = _git_porcelain()
    if not porcelain:
        return 0

    output = {
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": _build_message(porcelain, _git_shortstat()),
        }
    }
    json.dump(output, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main())
