# Add LSP Support for a New Language

Wire static analysis and symbol navigation for a new language into the LaaS development
toolchain — config, Find-Symbol wrapper, lsp-on-edit dispatch, and lsp.md documentation.

Role: operational · planning
Governs: .claude/hooks/lsp-on-edit.sh, .claude/skills/lsp.md, .claude/brain-registry.json

Persona: architect

Trigger phrases: "add lsp for", "wire up language server", "add type checking for",
Domain: deploy
Methodology-step: skills
"new language support", "lsp for rust", "lsp for go", "add pyright", "add tsserver",
"add language server", "type checking for new language", "static analysis for"

---

## Why This Matters

Adding LSP support for a language is a platform-wide structural decision — every agent
working on files of that type benefits from edit-time feedback and AST-accurate symbol
search. Half-wired support (e.g., config without a Find-Symbol script, or a script without
`lsp-on-edit.sh` dispatch) is worse than no support because it creates the illusion of
coverage. The steps below are designed to be all-or-nothing: complete all six steps or
document explicitly why a step was skipped.

The pattern follows **Everything is Code** from `devops-philosophy.md`. Static analysis
is not a manual review step — it runs automatically at edit time and is queryable on demand.

---

## Pre-Flight Checklist

Before starting, confirm these four conditions are true:

- [ ] **Language server exists** for this language (pyright, rust-analyzer, gopls, etc.)
- [ ] **CLI-invocable**: the language server can be run from a shell command (`npx pyright`, `gopls check`, etc.)
- [ ] **Structured output available**: the CLI can produce JSON or parseable output (`--outputjson`, `-json`, etc.)
- [ ] **File extension is distinct**: the extension doesn't conflict with an existing language (`.py`, `.go`, `.rs`)

If structured output is not available, the Find-Symbol wrapper can fall back to regex-based parsing
(as `Find-PySymbol.ps1` does for definition/reference search). Document the limitation in `lsp.md`.

---

## Step 1 — Create the Language Config File

Create the language server config in the relevant source directory.

**Pattern:** config file lives at the root of the language's source tree.

| Language | Config file | Location |
|----------|-------------|----------|
| Python | `pyrightconfig.json` | `apps/api/` |
| TypeScript | `tsconfig.json` | `apps/<app>/` |
| Go | `.golangci.yml` | repo root or `apps/<service>/` |
| Rust | `rust-analyzer` uses `Cargo.toml` | workspace root |

**For Python (already done):** `apps/api/pyrightconfig.json` exists with `typeCheckingMode: "basic"`.

**For Shell/Bash (already done):** `Find-ShellSymbol.ps1` uses `shellcheck --format=json1` for
linting and text regex for function search. No config file needed — shellcheck auto-detects.

**Principle:** Start with the permissive mode (`basic`, not `strict`). Strict mode on an unannotated
codebase produces hundreds of warnings that block adoption. Ramp up strictness after annotations are added.

---

## Step 2 — Create `Find-<Lang>Symbol.ps1`

Create a new PowerShell wrapper in `scripts/dev/` that mirrors the existing pattern.

**Naming convention:** `Find-<Lang>Symbol.ps1` where `<Lang>` is PascalCase language name.

| Existing wrappers | Language |
|-------------------|----------|
| `Find-Symbol.ps1` | PowerShell |
| `Find-TFSymbol.ps1` | Terraform |
| `Find-TSSymbol.ps1` | TypeScript |
| `Find-PySymbol.ps1` | Python |

**Required operations the wrapper must support:**

```powershell
# Full check — runs language server, prints structured error/warning summary
Find-<Lang>Symbol.ps1 -Check

# Symbol search — finds definitions, imports, call sites
Find-<Lang>Symbol.ps1 -Name <symbol> [-Type function|class|variable]

# Single file check
Find-<Lang>Symbol.ps1 -File <path>
```

**Implementation notes:**

- Use `--outputjson` (or equivalent) for structured output; fall back to line-by-line parsing
- For symbol search: use language server output if it exposes call sites; otherwise use regex patterns
  over source files (look for definition patterns, import patterns, and call patterns separately)
- Color-code output: green = definition, cyan = class, blue = variable, gray = import, white = call
- Include `-ApiDir` (or equivalent) parameter to override the source directory for testing

See `Find-PySymbol.ps1` as the canonical reference implementation.

---

## Step 3 — Add Dispatch Block to `lsp-on-edit.sh`

Edit `.claude/hooks/lsp-on-edit.sh` in three places:

**3a. Add the extension to the supported extensions case:**

```bash
# Before:
case "$EXT" in
  ps1|tf|ts|tsx|py) ;;

# After (example: adding Go):
case "$EXT" in
  ps1|tf|ts|tsx|py|go) ;;
```

**3b. Add a `run_<lang>()` function** after the existing language functions and before the dispatch block.
Follow the pattern of `run_pyright()`:

```bash
run_<lang>() {
  local file="$1"
  local display_name
  display_name=$(basename "$file")

  # 1. Find the language root (directory containing the config file)
  local lang_dir
  lang_dir=$(python3 -c "..." 2>/dev/null || true)

  if [[ -z "$lang_dir" ]]; then
    echo "[lsp:<lang>] $display_name: no <config-file> found — skipping"
    return
  fi

  # 2. Run the language server
  local output exit_code
  output=$(cd "$lang_dir" && <lsp-command> 2>/dev/null) || exit_code=$?

  # 3. Extract counts from JSON output
  local err_count warn_count
  err_count=$(echo "$output" | python3 -c "..." 2>/dev/null || echo "0")
  warn_count=$(echo "$output" | python3 -c "..." 2>/dev/null || echo "0")

  # 4. Log to lsp-on-edit.log
  { echo "=== lsp-on-edit [<lang>] $display_name ($TIMESTAMP) ==="; echo "$output"; echo ""; } >> "$LOG_FILE"

  # 5. Emit summary to context
  if [[ "$err_count" -eq 0 && "$warn_count" -eq 0 ]]; then
    echo "[lsp:<lang>] $display_name: OK"
  else
    echo "[lsp:<lang>] $display_name: $err_count error(s), $warn_count warning(s)"
    # Print up to 5 diagnostics
  fi
}
```

**3c. Add the dispatch case:**

```bash
case "$EXT" in
  # ... existing cases ...
  <ext>)
    run_<lang> "$FILE_PATH_NORM" &
    ;;
esac
```

---

## Step 4 — Add Language Section to `lsp.md`

Add a new `## Step N — <Language>: <LSP Name> and Symbol Search` section to `.claude/skills/lsp.md`.

**Required subsections:**

```markdown
## Step N — <Language>: <LSP Name> and Symbol Search

### Check all type errors

[command example with output]

### Find a symbol

[Find-<Lang>Symbol.ps1 -Name examples with output]

### Check a single file

[Find-<Lang>Symbol.ps1 -File example]

### Configuration

[Explain config file location and mode choice. Document why you chose this strictness level.]

### Known limitations

[Document what the CLI doesn't expose vs. what the IDE exposes. Be explicit about regex fallbacks.]
```

Also update:

1. **Quick Reference table** — add rows for the new language
2. **File Read Protocol table** — add the new file type row
3. **lsp-on-edit Hook section** — add the new extension to the bullet list and example output

---

## Step 5 — Test the Integration

Verify the full pipeline before declaring the work complete.

**5a. Edit-time hook:**
```bash
# Create a file with a deliberate type error, then edit it
# Verify lsp-on-edit.log contains a [lsp:<lang>] entry
grep "<lang>" .claude/lsp-on-edit.log | tail -5
```

**5b. Find-Symbol wrapper:**
```powershell
# Run Check mode — should return error count
pwsh -File scripts/dev/Find-<Lang>Symbol.ps1 -Check

# Run symbol search — should find at least one result in the source
pwsh -File scripts/dev/Find-<Lang>Symbol.ps1 -Name <known-symbol>
```

**5c. Shellcheck:**
```bash
shellcheck --severity=warning .claude/hooks/lsp-on-edit.sh
```

**5d. Pester gate:**
```powershell
# If you added test coverage for the new script
pwsh -File scripts/verify/run-tests.ps1 -Target dev
```

---

## Step 6 — Update `suggest-lsp-for-grep.sh` Advisory

After wiring the new language, add the new `Find-<Lang>Symbol.ps1` script to the advisory output
in `.claude/hooks/suggest-lsp-for-grep.sh`.

Find the advisory echo block and add a line:

```bash
echo "   <Language>:    pwsh -File scripts/dev/Find-<Lang>Symbol.ps1 -Name \"${GREP_PATTERN}\""
```

This ensures the grep-detection hook surfaces the new wrapper when code-navigation greps are detected.

---

## File Checklist

Files to create:

| File | What |
|------|------|
| `<source-root>/<lang-config>` | Language server config (e.g., `pyrightconfig.json`, `.golangci.yml`) |
| `scripts/dev/Find-<Lang>Symbol.ps1` | Symbol search + type check wrapper |

Files to modify:

| File | Change |
|------|--------|
| `.claude/hooks/lsp-on-edit.sh` | Add extension to case, add `run_<lang>()`, add dispatch |
| `.claude/skills/lsp.md` | Add language section, update Quick Reference + File Read Protocol |
| `.claude/hooks/suggest-lsp-for-grep.sh` | Add new wrapper to advisory output |
| `.claude/skills/skill-hook-pairs.md` | No new hook needed — `lsp-on-edit.sh` already covers the new language |
| `CLAUDE.md` | No change needed unless a new skill was added |

---

## See Also

- `lsp.md` — full toolkit reference for all currently wired languages
- `skill-hook-pairs.md` — `lsp-on-edit.sh` is already registered; no new hook entry needed for new languages
- `write-skill.md` — if you add a new companion skill for the language
- `add-new-app.md` — if the language is for a new Cloud Run service being added to the platform
