#!/usr/bin/env python3
"""PreToolUse hook that blocks git commit messages containing
Co-Authored-By signatures.

Rule:
    コミットメッセージのフッターに Claude の署名（Co-Authored-By）を付けないこと
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


def main() -> int:
    command = _load_command()
    if not command:
        return 0

    if not re.search(r"\bgit\b.*\bcommit\b", command):
        return 0

    if re.search(r"Co-Authored-By", command, re.IGNORECASE):
        sys.stderr.write(
            "❌ Co-Authored-By signatures in commit messages are forbidden.\n"
            "   Remediation: remove the Co-Authored-By line.\n"
            f"   Offending command: {command}\n"
        )
        return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
