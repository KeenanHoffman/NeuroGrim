#!/bin/bash
# PostToolUse hook — record skill invocations to the Brain's ledger.
#
# Captures TWO event subtypes:
#
#   subtype: "hard" — explicit Skill-tool invocations
#     (matcher: "Skill" in .claude/settings.local.json)
#   subtype: "soft" — Read-tool reads of a SKILL.md file
#     (matcher: "Read" in .claude/settings.local.json)
#
# The "hard" path captures /skill-name slash commands and any
# explicit `Skill(name=...)` tool calls. The "soft" path captures
# the more common pattern where an agent (or operator) reads a
# SKILL.md file directly to follow its guidance — without going
# through the Skill tool. Without this signal the invocation
# ledger systematically under-counts skill usage by an order of
# magnitude.
#
# Contract (verified 2026-04-22 against Claude Code 2.1.111):
#   - stdin delivers the PostToolUse JSON envelope.
#   - On Windows Git Bash, `/dev/stdin` doesn't exist — use plain `cat`.
#   - Fire-and-forget: always exit 0 so a hook failure never interrupts
#     the agent (PostToolUse can't block anyway).
#   - Captures ONLY: name + timestamp + session_id + invocation_id +
#     subtype. No arguments, no tool_response content, no transcript.
#
# Appends one JSONL line per matched event to:
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

# Common envelope fields.
session="${env_json##*\"session_id\":\"}"
session="${session%%\"*}"
invocation="${env_json##*\"tool_use_id\":\"}"
invocation="${invocation%%\"*}"
tool_name="${env_json##*\"tool_name\":\"}"
tool_name="${tool_name%%\"*}"

[ -z "${session}" ] && session="<unknown>"
[ -z "${invocation}" ] && invocation="<unknown>"

ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Branch on tool name. Unrecognized tools exit silently — this hook
# only cares about Skill (hard) and Read (soft).
case "$tool_name" in
  Skill)
    # Hard invocation: explicit Skill tool call.
    skill="${env_json##*\"skill\":\"}"
    skill="${skill%%\"*}"
    [ -z "${skill}" ] && skill="<unknown>"
    subtype="hard"
    ;;
  Read)
    # Soft invocation: only count Reads of SKILL.md files. Path
    # patterns we recognize:
    #   .claude/skills/<name>/SKILL.md  (plugin format)
    #   .claude/skills/<name>.md        (legacy format)
    # Anything else exits silently.
    file_path="${env_json##*\"file_path\":\"}"
    file_path="${file_path%%\"*}"

    # Normalize Windows backslashes to forward slashes so the
    # matchers below are platform-independent.
    file_path="${file_path//\\/\/}"

    # Plugin: .../.claude/skills/<name>/SKILL.md
    if [[ "$file_path" == *"/.claude/skills/"*"/SKILL.md" ]]; then
      tail="${file_path##*/.claude/skills/}"
      skill="${tail%%/SKILL.md}"
    # Legacy: .../.claude/skills/<name>.md  (NOT in a subdir)
    elif [[ "$file_path" == *"/.claude/skills/"*".md" ]]; then
      tail="${file_path##*/.claude/skills/}"
      # Reject paths that have a `/` after the prefix (those are
      # plugin internals like `/.claude/skills/foo/REFERENCE.md`,
      # not a top-level legacy SKILL).
      case "$tail" in
        */*) exit 0 ;;
      esac
      skill="${tail%.md}"
      # README.md / archived/ etc. are not skills.
      case "$skill" in
        README*|archived|.*) exit 0 ;;
      esac
    else
      # Not a SKILL.md path — exit silently.
      exit 0
    fi
    [ -z "${skill}" ] && exit 0
    subtype="soft"
    ;;
  *)
    # Tool we don't track. Exit silently.
    exit 0
    ;;
esac

# Append one JSONL line. schema_version: "2" (added subtype field).
printf '{"schema_version":"2","ts":"%s","type":"skill","subtype":"%s","name":"%s","session_id":"%s","invocation_id":"%s"}\n' \
  "$ts" "$subtype" "$skill" "$session" "$invocation" >> "$ledger"

exit 0
