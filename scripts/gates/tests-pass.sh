#!/usr/bin/env bash
# v4.0 publish-gate helper: run the workspace's quiet test wrapper
# via the local release binary. Why a wrapper script:
#
# 1. The gate runner uses `cmd /C` on Windows; cmd doesn't grok
#    `bash -c '...'` with the single-quote grouping that `./relative
#    /path` invocations need. A wrapper script sidesteps shell
#    differences — bash explicitly invoked, the rest is POSIX.
#
# 2. Use the local release binary at `target/release/neurogrim`
#    rather than the PATH-installed `neurogrim`. A long-running
#    federation peer or a pre-existing `neurogrim` install often
#    holds the PATH binary at an older version; pointing at the
#    local target bypasses both issues.
#
# Exit code mirrors `neurogrim test`: 0 all-pass, 1 any-fail.
set -uo pipefail

cd neurogrim

# `neurogrim-ecosystem/tests/contract.rs` shells out to a built
# example at `target/debug/examples/stub_child_brain.exe`. cargo's
# default test invocation builds examples with hash-suffixed names
# (e.g. `stub_child_brain-<hash>.exe`); only an explicit
# `cargo build --example` produces the canonical name the test
# expects. Build it eagerly so the contract suite has a stable path.
cargo build --example stub_child_brain >/dev/null 2>&1 || true

./target/release/neurogrim test --project-root ..
