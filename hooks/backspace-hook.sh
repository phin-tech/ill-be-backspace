#!/usr/bin/env bash
# backspace: ignore[block-too-long, comment-code-ratio] — this header is
# install documentation, not commentary on the script below.
# Claude Code PostToolUse hook: reports over-long comments the moment they are
# written, rather than waiting for a commit.
#
# Install by adding to .claude/settings.json:
#
#   {
#     "hooks": {
#       "PostToolUse": [{
#         "matcher": "Write|Edit",
#         "hooks": [{
#           "type": "command",
#           "command": "${CLAUDE_PROJECT_DIR}/hooks/backspace-hook.sh",
#           "timeout": 15
#         }]
#       }]
#     }
#   }
#
# Environment:
#   BACKSPACE_BIN    binary to run (default: backspace on PATH)
#   BACKSPACE_BLOCK  set to 1 to stop Claude until the comment is addressed;
#                    by default the finding is surfaced as context and Claude
#                    decides, because comment length is a judgement call
set -uo pipefail

input=$(cat)
file=$(jq -r '.tool_input.file_path // empty' <<<"$input")
cwd=$(jq -r '.cwd // empty' <<<"$input")

# Nothing to check: a tool call without a file, or a file that no longer exists.
[ -n "$file" ] && [ -f "$file" ] || exit 0

bin="${BACKSPACE_BIN:-}"
if [ -z "$bin" ]; then
  # A local release build wins, so working on backspace exercises the code
  # under development rather than the installed version.
  local_build="${CLAUDE_PROJECT_DIR:-.}/target/release/backspace"
  if [ -x "$local_build" ]; then bin="$local_build"; else bin="backspace"; fi
fi
command -v "$bin" >/dev/null 2>&1 || [ -x "$bin" ] || exit 0

cd "${cwd:-$(dirname "$file")}" 2>/dev/null || exit 0

# Prefer --diff so only comments in uncommitted work are reported. That scopes
# the check to what this session actually wrote, instead of every legacy comment
# in a file Claude happened to touch.
if git rev-parse --git-dir >/dev/null 2>&1; then
  report=$("$bin" "$file" --diff --json 2>/dev/null)
else
  report=$("$bin" "$file" --all --json 2>/dev/null)
fi
[ -n "$report" ] || exit 0

# A `note` says MAY leave as is. Interrupting to relay one would contradict it,
# so only findings that ask for a change are worth a round trip.
report=$(jq '.violations |= map(select(.severity != "note"))
             | .summary.violations = (.violations | length)' <<<"$report")
count=$(jq -r '.summary.violations // 0' <<<"$report")
[ "$count" -gt 0 ] 2>/dev/null || exit 0

message=$(jq -r '
  "backspace flagged \(.summary.violations) comment\(if .summary.violations == 1 then "" else "s" end) you just wrote:\n" +
  ([.violations[] | "  \(.file):\(.start_line) [\(.rule)] \(.message)\n    > \(.comment[0] // "" | .[0:100])"] | join("\n")) +
  "\n\nKeep the invariant a reader cannot derive from the code. Move history, dates and rejected approaches into the commit message. If the length is genuinely warranted, add `backspace: ignore[rule] — reason`."
' <<<"$report")

if [ "${BACKSPACE_BLOCK:-0}" = "1" ]; then
  jq -n --arg reason "$message" '{decision: "block", reason: $reason}'
else
  jq -n --arg ctx "$message" '{
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: $ctx
    }
  }'
fi
exit 0
