#!/usr/bin/env python3
"""PreToolUse hook that blocks `git -C <path>` usage.

Rule:
    git コマンドで `-C <path>` オプションを使わないこと。
    代わりに `cd <path>` してから git コマンドを実行する。
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


def _strip_quoted_spans(command: str) -> str:
    """Remove single- and double-quoted spans so we don't match inside
    commit messages or echo strings."""
    return re.sub(r"'[^']*'|\"[^\"]*\"", "", command)


def main() -> int:
    command = _load_command()
    if not command:
        return 0

    stripped = _strip_quoted_spans(command)
    if re.search(r"\bgit\s+-C\s", stripped):
        sys.stderr.write(
            "❌ Using `git -C <path>` is forbidden in this project.\n"
            "   Remediation: use `cd <path>` first, then run git commands.\n"
            f"   Offending command: {command}\n"
        )
        return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
