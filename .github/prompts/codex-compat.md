# Codex CLI Compatibility Analysis

You are analyzing new Codex CLI releases to determine whether the **Canopy** desktop application (Electron + Svelte 5) needs code changes to stay compatible or to adopt new features.

## Context

The workflow has provided these values at the top of the prompt:

- **FROM_VERSION** — the last version we checked (exclusive lower bound, e.g. `rust-v0.118.0`)
- **TO_VERSION** — the latest available stable release (inclusive upper bound)
- **EXISTING_PR** — PR number if a compatibility PR is already open (empty if none)
- **REPO** — this repository (owner/repo format)

The **release repo** is `openai/codex`. Tags use the format `rust-v{version}`.

The **target branch** for PRs is `chore/codex-compat`.

Release notes for each new version are appended below under `## Release Notes`.

## Your task

### 1. Understand the releases

Read the release notes provided below. For each version, identify:

- New or changed **hook events** (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop) and their input/output JSON schemas
- New or changed **CLI flags** (`--model`, `--ask-for-approval`, `--sandbox`, `--full-auto`, `--enable`, `--profile`, etc.)
- Changed **config format** (`hooks.json` structure, `config.toml` features section)
- New **sandbox modes** or **approval modes**
- New **tool types** beyond Bash in PreToolUse/PostToolUse events
- Changes to **`codex resume`** subcommand behavior
- New or changed **environment variables**
- Changes to how hooks receive input on stdin or return output on stdout

### 2. Fetch detailed diffs

For deeper analysis, compare tags in the release repo:

```bash
# Compare two tags to see all changed files
gh api repos/openai/codex/compare/{FROM_VERSION}...{TO_VERSION} --jq '.files[] | "\(.filename) (\(.status))"' | head -100

# Read hooks schema or config modules at a given tag
gh api "repos/openai/codex/contents/codex-rs/hooks?ref={TO_VERSION}" --jq '.[] | .name'
```

Focus on changes in `codex-rs/hooks/`, `codex-rs/config/`, `codex-rs/cli/`, and any files mentioning `hook`, `approval`, `sandbox`, or `config`.

### 3. Scan the Canopy codebase

**Known integration points** (start here):

- `src/main/agents/adapters/codex.ts` — adapter: hook events, event normalization, CLI args, env vars, hooks.json setup, resume args
- `src/renderer/src/components/preferences/CodexPrefs.svelte` — preference options (model, approval mode, sandbox, full-auto, profile, API key, base URL)
- `src/renderer/src/components/agents/CodexExtras.svelte` — inspector extras (cwd, turns, transcript, last response)
- `src/renderer/src/lib/agents/agentState.svelte.ts` — agent state handling, extra data merging
- `src/main/agents/types.ts` — AgentType union, NormalizedEventName, AgentAdapter interface

**Then discover more** — search broadly for additional references:

```bash
grep -r "codex" --include="*.ts" --include="*.svelte" --include="*.yml" --include="*.json" -l .
grep -r "openai" --include="*.ts" --include="*.json" -l .
```

### 4. Apply changes

Be **proactive** — not just compatibility fixes but also:

- Add new hook events if Codex introduces them (update `CODEX_HOOK_EVENTS`, `EVENT_MAP`, and `busyEvents`/`idleEvents` in `codex.ts`)
- Add new CLI flags to `buildCliArgs` and corresponding preference fields in `CodexPrefs.svelte`
- Update `normalizeEvent` if new input fields appear in hook stdin JSON
- Add new approval/sandbox mode options to preference dropdowns
- Extract new data from hooks into `extra` field for the inspector
- Update `toNotchStatus` if new status transitions become available

For each change, make a **targeted, minimal edit**. Do not reformat or restructure code beyond what the change requires.

### 5. Create or update the PR

Use the FROM_VERSION, TO_VERSION, and EXISTING_PR values from the prompt header.

**If no existing PR** (EXISTING_PR is empty):

1. Create the branch: `git checkout -b chore/codex-compat`
2. Commit changes with descriptive messages (one commit per logical change group, use `chore:` or `fix:` prefix)
3. Push: `git push origin chore/codex-compat`
4. Create PR targeting `next` with this structure:

```
Title: chore(deps): codex compatibility update ({FROM_VERSION} → {TO_VERSION})

Body:
## Codex CLI Compatibility Update

### Versions analyzed
[List each version analyzed]

### Relevant changes
[For each version: key changes that affected our codebase]

### Modifications made
[For each file changed: what was modified and why]

### Risk assessment
[Low/Medium/High — explain any risks or breaking changes]
```

**If existing PR** (EXISTING_PR is a PR number):

1. Checkout the existing branch: `git fetch origin chore/codex-compat && git checkout chore/codex-compat`
2. Commit incremental changes
3. Push: `git push origin chore/codex-compat`
4. Update the PR title and description to cover the expanded version range using `gh pr edit`

### 6. If no changes needed

If after analysis you determine no code changes are required:

1. Do NOT create a branch or PR
2. Write a summary to `$GITHUB_STEP_SUMMARY`:

```bash
cat >> "$GITHUB_STEP_SUMMARY" <<'EOF'
## Codex CLI Compatibility Check

Analyzed versions: {FROM_VERSION} → {TO_VERSION}

**No code changes needed.** The release changes do not affect Canopy's integration.
EOF
```

## Tone

Be precise and factual. State what you found, what you changed, and why. No filler or commentary beyond what is needed to explain each decision.
