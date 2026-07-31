#!/usr/bin/env bash
# backspace: ignore[block-too-long, comment-code-ratio] — this header is
# install documentation, not commentary on the script below.
# Claude Code Stop hook: checks the assistant's own message against the same
# word list that governs comments, so a word you never want to read does not
# reach you through chat either.
#
# Install by adding to .claude/settings.json:
#
#   {
#     "hooks": {
#       "Stop": [{
#         "hooks": [{
#           "type": "command",
#           "command": "${CLAUDE_PROJECT_DIR}/hooks/backspace-chat-hook.sh",
#           "timeout": 10
#         }]
#       }]
#     }
#   }
#
# Environment:
#   BACKSPACE_BIN         binary to run (default: backspace on PATH)
#   BACKSPACE_CHAT_BLOCK  set to 1 to make Claude keep working until it has
#                         reworded. Off by default: a Stop block forces another
#                         turn rather than editing the message you already read,
#                         so it costs a round trip to fix wording after the fact.
set -uo pipefail

input=$(cat)
message=$(jq -r '.last_assistant_message // empty' <<<"$input")
cwd=$(jq -r '.cwd // empty' <<<"$input")
[ -n "$message" ] || exit 0

# Never re-enter: a blocked Stop triggers another turn, which triggers this hook.
[ "$(jq -r '.stop_hook_active // false' <<<"$input")" = "true" ] && exit 0

bin="${BACKSPACE_BIN:-}"
if [ -z "$bin" ]; then
  # A local release build wins, so working on backspace exercises the code
  # under development rather than the installed version.
  local_build="${CLAUDE_PROJECT_DIR:-.}/target/release/backspace"
  if [ -x "$local_build" ]; then bin="$local_build"; else bin="backspace"; fi
fi
command -v "$bin" >/dev/null 2>&1 || [ -x "$bin" ] || exit 0
cd "${cwd:-.}" 2>/dev/null || exit 0

report=$(printf '%s' "$message" | "$bin" prose --json 2>/dev/null)
[ -n "$report" ] || exit 0

count=$(jq -r '.summary.violations // 0' <<<"$report")
[ "$count" -gt 0 ] 2>/dev/null || exit 0

detail=$(jq -r '[.violations[] | "  line \(.start_line): \(.message)"] | join("\n")' <<<"$report")

if [ "${BACKSPACE_CHAT_BLOCK:-0}" = "1" ]; then
  jq -n --arg r "Your message used wording the user has banned:
$detail

Say it again without those words." '{decision: "block", reason: $r}'
else
  jq -n --arg ctx "backspace: your last message used banned wording:
$detail

Avoid these words in future replies." '{
    hookSpecificOutput: {
      hookEventName: "Stop",
      additionalContext: $ctx
    }
  }'
fi
exit 0
