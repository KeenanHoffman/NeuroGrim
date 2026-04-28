#!/bin/bash
# PostToolUse hook — record Skill tool invocations to the Brain's ledger.
#
# Installed via a PostToolUse hook matching "Skill" in
# `.claude/settings.local.json`. See `docs/invocation-ledger.md` for
# setup instructions.
#
# Contract (verified 2026-04-22 against Claude Code 2.1.111):
#   - stdin delivers the PostToolUse JSON envelope.
#   - On Windows Git Bash, `/dev/stdin` doesn't exist — use plain `cat`.
#   - Fire-and-forget: always exit 0 so a hook failure never interrupts
#     the agent (PostToolUse can't block anyway).
#   - Captures ONLY: name + timestamp + session_id + invocation_id.
#     No arguments, no tool_response content, no transcript content.
#
# Appends one JSONL line per invocation to:
#   $CLAUDE_PROJECT_DIR/.claude/brain/invocation-ledger.jsonl
#
# The ledger is gitignored (runtime state per existing Brain convention).

set -u

# Guardrail (concern C1): require CLAUDE_PROJECT_DIR. Without it the
# ledger would silently append to $PWD — whichever directory Claude
# Code happened to launch the hook from. That scatters ledger data
# across unrelated directories and makes `capability-hygiene`'s usage
# classification silently unreliable. Fail loud instead.
if [ -z "${CLAUDE_PROJECT_DIR:-}" ]; then
  echo "record-skill-invocation.sh: CLAUDE_PROJECT_DIR is unset; refusing to write ledger to an unknown directory." >&2
  exit 1
fi

cd "$CLAUDE_PROJECT_DIR"
mkdir -p .claude/brain
ledger=".claude/brain/invocation-ledger.jsonl"

# Read full stdin envelope (Git Bash on Windows: plain `cat`, not /dev/stdin).
env_json="$(cat)"

# Parameter-expansion extraction — avoids a jq / python dependency.
# Skill names are `[a-z0-9:-]+` per Claude Code convention, no quotes/escapes.
skill="${env_json##*\"skill\":\"}"
skill="${skill%%\"*}"
session="${env_json##*\"session_id\":\"}"
session="${session%%\"*}"
invocation="${env_json##*\"tool_use_id\":\"}"
invocation="${invocation%%\"*}"

# Fallbacks if extraction failed (e.g. malformed envelope — logged as empty).
[ -z "${skill}" ] && skill="<unknown>"
[ -z "${session}" ] && session="<unknown>"
[ -z "${invocation}" ] && invocation="<unknown>"

ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Append one JSONL line. schema_version: "1".
printf '{"schema_version":"1","ts":"%s","type":"skill","name":"%s","session_id":"%s","invocation_id":"%s"}\n' \
  "$ts" "$skill" "$session" "$invocation" >> "$ledger"

exit 0
