#!/usr/bin/env python3
"""PreToolUse hook that enforces CLAUDE.md's prohibition on chaining Bash
commands with `&&`.

Background
----------
`.claude/CLAUDE.md` contains the rule:

    複数のコマンドを `&&` で連結しないこと。代わりに個別の Bash ツール呼び出しを使う

Until now the rule was enforced only by the agent reading the file.  That
is unreliable — any lapse of attention silently violates the project's
policy.  This hook promotes the rule to a hard system-level check: when a
`Bash` tool call contains `&&` between commands the hook exits with code
`2`, which tells the Claude Code harness to block the tool call and surface
the stderr back to the agent.

Detection strategy
------------------
The literal character sequence `&&` can appear legitimately inside a
quoted string (for example `echo "run a && b"`) or inside a `[[ … ]]`
conditional.  To keep false positives manageable the hook:

* removes balanced single- and double-quoted spans before inspecting the
  command, so `echo "a && b"` is allowed;
* only flags occurrences surrounded by whitespace, matching the way the
  shell requires `&&` to be written when chaining commands.

The hook is intentionally conservative: if something unusual (e.g. a here
document, an escaped `&&`) is misdetected, the user can bypass it by
splitting the command into multiple `Bash` tool calls — which is exactly
what the CLAUDE.md rule asks for in the first place.
"""

from __future__ import annotations

import json
import re
import sys


QUOTED_SPAN = re.compile(r"'[^']*'|\"[^\"]*\"")
AND_CHAIN = re.compile(r"\s&&\s")


def _strip_quoted_spans(command: str) -> str:
    """Replace quoted substrings with empty placeholders of the same shape."""
    return QUOTED_SPAN.sub(lambda m: m.group(0)[0] * 2, command)


def _load_command() -> str:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        # If we cannot parse the hook input, do not block the tool call —
        # fail open rather than lock the agent out of Bash entirely.
        return ""
    tool_input = payload.get("tool_input") or {}
    return tool_input.get("command", "") or ""


def main() -> int:
    command = _load_command()
    if not command:
        return 0

    if AND_CHAIN.search(_strip_quoted_spans(command)):
        sys.stderr.write(
            "❌ Bash command chaining with `&&` is forbidden in this project.\n"
            "   Rule: .claude/CLAUDE.md — "
            "`複数のコマンドを && で連結しないこと`.\n"
            "   Remediation: issue each step as its own `Bash` tool call.\n"
            f"   Offending command: {command}\n"
        )
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
