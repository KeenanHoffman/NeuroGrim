#!/usr/bin/env python3
"""Supply-chain L2 vigilance strict-with-bypass gate.

Extracted from `scripts/prepublish-check.sh` per the 2026-04-26
PRE-RELEASE-ASSESSMENT C5 + C6 fixes. Embedding ~50 lines of
Python in a bash heredoc obscured the strict-parsing posture and
made finding-format edge cases invisible.

# What this checks

For each Layer 2 vigilance finding in the CMDB, verify that the
decision-ledger has a matching resolved entry (any of:
``review-triaged``, ``accept``, ``reject``, ``pin-to-last-good``).
The match key is ``(ecosystem, package_name, signal_kind)`` where
``signal_kind = "vigilance:<finding-kind>"``.

Findings of kind ``sensor-degradation`` are skipped (informational;
weight 0; not a triage candidate).

# Strict-parse posture (C5)

JSONL parse errors are FATAL — exit code 2. Previously the bash
heredoc silently `continue`d on malformed lines, masking
ledger-corruption failures. A corrupt ledger line could make a
real triage entry invisible to the gate; the gate would report
the finding as un-triaged, but the operator might think they
already triaged it. Strict-parse forces the operator to inspect
+ repair the ledger.

# Finding-format validation (C6)

Vigilance finding names follow ``<kind>:<eco>:<pkg>`` (3 parts,
colon-separated). Findings whose name doesn't match this format
are treated as ERRORS (exit 2), not silent skips. If vigilance
ever emits a different format we want to know — silent skipping
would mask real findings.

# Exit codes

* 0 — all findings have matching resolved ledger entries
* 1 — un-triaged findings present (operator action required)
* 2 — script error: malformed input, parse failure, or unknown
  finding name format

# Usage

    _supply-chain-bypass-check.py <vigilance-cmdb-path> <ledger-path>

The ledger path may not exist (no triage history yet); that's
treated as "no triaged entries" not as an error.
"""

from __future__ import annotations

import json
import os
import sys


# Match these kinds in the ledger as "this finding has been triaged".
RESOLVED_ENTRY_KINDS = frozenset(
    {"review-triaged", "accept", "reject", "pin-to-last-good"}
)

# Skip these finding kinds (informational; weight 0; not triage
# candidates). Keep in sync with VigilanceKind::SensorDegradation
# in supply_chain_vigilance/scoring.rs.
SKIPPED_FINDING_KINDS = frozenset({"sensor-degradation"})


def _die(msg: str, exit_code: int = 2) -> None:
    """Exit with stderr message + non-zero code."""
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(exit_code)


def _read_vig_cmdb(path: str) -> list[dict]:
    """Read the vigilance CMDB; return the findings array.

    Raises (via _die) if the file doesn't parse or doesn't contain
    a `findings` key.
    """
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    except FileNotFoundError:
        _die(f"vigilance CMDB not found at {path}")
    except json.JSONDecodeError as e:
        _die(f"vigilance CMDB at {path} is not valid JSON: {e}")
    findings = data.get("findings")
    if findings is None:
        _die(f"vigilance CMDB at {path} missing required 'findings' array")
    if not isinstance(findings, list):
        _die(
            f"vigilance CMDB at {path} 'findings' must be an array; "
            f"got {type(findings).__name__}"
        )
    return findings


def _read_ledger_triaged_set(path: str) -> set[tuple[str, str, str]]:
    """Walk the JSONL ledger; collect (eco, name, signal_kind) tuples
    for any resolved entry.

    Strict-parse posture: a malformed JSONL line is fatal (exit 2),
    not silently skipped. The operator must inspect + repair the
    ledger before the gate can pass.
    """
    triaged: set[tuple[str, str, str]] = set()
    if not os.path.exists(path):
        # No triage history yet — first-run posture. Empty set.
        return triaged

    with open(path, encoding="utf-8") as f:
        for lineno, raw_line in enumerate(f, start=1):
            line = raw_line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError as e:
                _die(
                    f"ledger {path} line {lineno} is not valid JSON: {e}. "
                    f"Inspect + repair the ledger before re-running. "
                    f"(strict-parse posture, 2026-04-26 C5 fix)"
                )
            kind = entry.get("entry_kind")
            if kind not in RESOLVED_ENTRY_KINDS:
                continue
            pkg = entry.get("package") or {}
            eco = pkg.get("ecosystem", "")
            name = pkg.get("name", "")
            for sig in entry.get("triggering_signals") or []:
                sk = sig.get("signal_kind", "")
                if eco and name and sk:
                    triaged.add((eco, name, sk))
    return triaged


def _categorize_findings(findings: list[dict]) -> tuple[list, list, list]:
    """Split findings into (countable, sensor_degradation, malformed).

    Vigilance finding names follow `<kind>:<eco>:<pkg>` (3 parts,
    colon-separated). Returned tuples have shape:

    * countable: (kind, eco, pkg_name) for findings that need a
      matching ledger entry.
    * sensor_degradation: (name,) for findings to skip (info-only).
    * malformed: (name,) for findings that didn't match the contract.

    Per C6: malformed findings are SURFACED, not silently dropped.
    """
    countable: list[tuple[str, str, str]] = []
    sensor_degradation: list[tuple[str]] = []
    malformed: list[tuple[str]] = []
    for f in findings:
        name = f.get("name", "")
        parts = name.split(":", 2)
        if len(parts) != 3:
            malformed.append((name,))
            continue
        kind, eco, pkg_name = parts
        if kind in SKIPPED_FINDING_KINDS:
            sensor_degradation.append((name,))
            continue
        countable.append((kind, eco, pkg_name))
    return countable, sensor_degradation, malformed


def main() -> int:
    if len(sys.argv) != 3:
        _die(
            "usage: _supply-chain-bypass-check.py "
            "<vigilance-cmdb-path> <ledger-path>"
        )

    vig_path = sys.argv[1]
    ledger_path = sys.argv[2]

    findings = _read_vig_cmdb(vig_path)
    triaged = _read_ledger_triaged_set(ledger_path)

    countable, _sd, malformed = _categorize_findings(findings)

    if malformed:
        # C6: don't silently swallow unknown finding-name formats.
        # If vigilance ever emits something other than
        # `<kind>:<eco>:<pkg>` we want the gate to break loudly.
        msg_lines = [
            f"{len(malformed)} finding(s) have unrecognized name format "
            f"(expected '<kind>:<ecosystem>:<package>'):",
        ]
        for (name,) in malformed[:20]:
            msg_lines.append(f"  {name!r}")
        if len(malformed) > 20:
            msg_lines.append(f"  ... and {len(malformed) - 20} more")
        msg_lines.append(
            "If this is intentional (new finding format), update "
            "scripts/_supply-chain-bypass-check.py to parse the new "
            "shape; otherwise treat as a vigilance-emit regression."
        )
        _die("\n".join(msg_lines))

    untriaged: list[tuple[str, str, str]] = []
    for kind, eco, pkg_name in countable:
        sig_key = f"vigilance:{kind}"
        if (eco, pkg_name, sig_key) not in triaged:
            untriaged.append((eco, pkg_name, kind))

    if untriaged:
        print("UNTRIAGED:")
        for eco, pkg, kind in untriaged:
            print(f"  {eco} {pkg} {kind}")
        return 1

    total = len(countable)
    print(f"all {total} findings have matching ledger entries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
