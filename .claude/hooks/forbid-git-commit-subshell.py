#!/usr/bin/env python3
"""PreToolUse hook that blocks git commit commands using $() command
substitution or piped input (echo ... | git commit -F -).

Rule:
    コミット時に `$()` コマンド置換や `echo ... | git commit -F -` を使わないこと
    （承認ダイアログが表示されるため）。代わりに `-m` オプションで直接渡す。
"""

from __future__ import annotations

import json
import re
import sys


def _load_command() -> str:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return ""
    tool_input = payload.get("tool_input") or {}
    return tool_input.get("command", "") or ""


def _strip_single_quoted_spans(command: str) -> str:
    """Remove single-quoted spans only.  Double-quoted spans still allow
    shell expansion ($(), variable substitution) so we must keep them."""
    return re.sub(r"'[^']*'", "", command)


def _has_git_commit(command: str) -> bool:
    return bool(re.search(r"\bgit\b.*\bcommit\b", command))


def main() -> int:
    command = _load_command()
    if not command:
        return 0

    if not _has_git_commit(command):
        return 0

    stripped = _strip_single_quoted_spans(command)

    if "$(" in stripped:
        sys.stderr.write(
            "❌ Using $() command substitution in git commit is forbidden.\n"
            "   Remediation: pass the message directly with -m option.\n"
            f"   Offending command: {command}\n"
        )
        return 2

    if re.search(r"\|\s*git\b.*\bcommit\b", stripped):
        sys.stderr.write(
            "❌ Piping input to git commit is forbidden.\n"
            "   Remediation: pass the message directly with -m option.\n"
            f"   Offending command: {command}\n"
        )
        return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
