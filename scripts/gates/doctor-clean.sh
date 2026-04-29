#!/usr/bin/env bash
# S12-G-7 publish-gate helper: run `neurogrim doctor` and treat
# warnings as non-fatal. Exit codes:
#   0 = doctor returned 0 (clean) or 1 (warnings only) → gate passes
#   1 = doctor returned 2+ (errors) → gate fails
#
# Why this exists: `neurogrim doctor`'s exit code is 0 clean / 1
# warn / 2 error. NeuroGrim's own brain currently has one advisory
# warning (`rust-health` advisory orphan — the sensor isn't authored
# yet), so doctor exits 1 even when the configuration is shippable.
# This wrapper flattens the "warnings OK, errors not" semantics into
# the gate's pass/fail signal.
#
# When the rust-health sensor lands (or rust-health is removed from
# domain_weights), this script becomes redundant and the gate can
# call `neurogrim doctor` directly.
set -uo pipefail

neurogrim doctor --registry .claude/brain-registry.json
rc=$?

if [ "$rc" -lt 2 ]; then
    exit 0
fi
exit 1
