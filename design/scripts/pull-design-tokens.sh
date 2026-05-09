#!/usr/bin/env bash
# Vendor design tokens from `neurogrim/design/system/` into a consumer repo.
#
# **STATUS (2026-05-09): SCAFFOLDING STUB.** The actual design system content
# has not yet moved into this directory — see
# `D:/Brains/NeuroGrim/.claude/plans/design-migration-execution.md`. Until
# step 2 of that plan executes, the source files live at
# `D:/local-pc-operational-management/children/neurogrim-ide/design/system/`
# and this script is a no-op that documents the intended I/O contract.
#
# Contract (post-migration, when this script becomes active):
# - Input:  D:/Brains/NeuroGrim/design/system/colors_and_type.css
#           (and any other system/ artifacts the consumer reads)
# - Output: <consumer-repo>/<consumer-token-path>
#           e.g., neurogrim-ide/src/styles/tokens.css
#
# Invocation shape (post-migration, draft):
#   ./pull-design-tokens.sh --consumer ide       --consumer-root /d/local-pc-operational-management/children/neurogrim-ide
#   ./pull-design-tokens.sh --consumer dashboard --consumer-root /d/Brains/NeuroGrim/neurogrim/crates/neurogrim-dashboard/frontend
#   ./pull-design-tokens.sh --consumer cli       --consumer-root /d/Brains/NeuroGrim/neurogrim/crates/neurogrim-cli
#
# Direction of flow: ONE-WAY READ. Consumers MUST NOT write back into
# the system/ tree. If the operator wants to tweak a brand-wide token,
# they edit `system/` directly; consumers re-pull on next vendor.
#
# Long-term replacement: a `neurogrim-design-tokens` crate / npm package
# would let consumers `cargo add` / `pnpm add` rather than scripting.
# Scripts are the v1 path; the crate is post-migration follow-up.

echo '[pull-design-tokens.sh] STUB — design system has not migrated yet.'
echo 'See D:/Brains/NeuroGrim/.claude/plans/design-migration-execution.md for the active execution plan.'
echo 'Until step 2 of that plan executes, the source files remain at neurogrim-ide/design/system/.'
exit 0
