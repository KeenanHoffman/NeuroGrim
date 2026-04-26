#!/usr/bin/env bash
# refresh-typosquat-list.sh — Refresh the static top-N popular-package
# lists used by the supply-chain-vigilance typosquat sensor.
#
# Usage:  bash scripts/refresh-typosquat-list.sh
#
# This script is operator-run, not automated. Quarterly cadence is
# the recommended baseline. After running, review the diff before
# committing — adversarial typosquat candidates can briefly enter
# popularity rankings. The list is a popularity signal, NOT a
# security signal; conservative changes preferred.
#
# Outputs (overwritten in place):
#   crates/neurogrim-sensory/data/typosquat-popular-cratesio.txt
#   crates/neurogrim-sensory/data/typosquat-popular-pypi.txt
#   crates/neurogrim-sensory/data/typosquat-popular-npm.txt
#
# Trust posture:
# - Each fetch is HTTPS to an established public source.
# - Output goes through human-review (operator + git commit) before
#   it reaches the trust chain.
# - This script is INTENTIONALLY a manual operator action — no cron,
#   no CI auto-update — to keep popularity-list updates auditable.
#
# E-SC-5 v1 (2026-04-25): script is a placeholder. The hand-curated
# seed lists shipped with v1 were authored manually. Fully-automated
# fetches against canonical sources land in a follow-on (B-2x).
# Until then, this script exists to document what the refresh process
# WILL look like and to give operators a starting point.

set -euo pipefail

DATA_DIR="$(cd "$(dirname "$0")/.." && pwd)/crates/neurogrim-sensory/data"

echo "Typosquat-list refresh — placeholder (E-SC-5 v1)"
echo "Data dir: $DATA_DIR"
echo
echo "v1 ships hand-curated seed lists. Automated refresh against"
echo "canonical sources (crates.io database dump / TopPyPI / npm-rank-list)"
echo "lands in a follow-on epic. Until then, edit the .txt files by hand"
echo "after consulting:"
echo
echo "  crates.io:  https://crates.io/api/v1/summary  +  https://crates.io/api/v1/crates?sort=downloads&per_page=100&page=1..N"
echo "  PyPI:       https://hugovk.github.io/top-pypi-packages/"
echo "  npm:        https://github.com/anvaka/npmrank or registry rank reports"
echo
echo "After editing, run unit tests to confirm nothing breaks:"
echo "  cargo test -p neurogrim-sensory --lib supply_chain_vigilance::typosquat"
echo
echo "And review the diff carefully before committing — adversarial"
echo "typosquat candidates can briefly enter popularity rankings."

exit 0
