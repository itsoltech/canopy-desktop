# OpenCode Compatibility Analysis

You are analyzing new OpenCode releases to determine whether the **Canopy** desktop application (Electron + Svelte 5) needs code changes to stay compatible or to adopt new features.

## Context

The workflow has provided these values at the top of the prompt:

- **FROM_VERSION** — the last version we checked (exclusive lower bound)
- **TO_VERSION** — the latest available release (inclusive upper bound)
- **EXISTING_PR** — PR number if a compatibility PR is already open (empty if none)
- **REPO** — this repository (owner/repo format)

The **release repo** is `anomalyco/opencode` — the official OpenCode repository.

The **target branch** for PRs is `chore/opencode-compat`.

Release notes for each new version are appended below under `## Release Notes`.

## Your task

### 1. Understand the releases

Read the release notes provided below. For each version, identify:

- New or changed **CLI flags** and **subcommands**
- Changed or removed **environment variables** and **config keys** (`OPENCODE_CONFIG_DIR`, `OPENCODE_CONFIG_CONTENT`, etc.)
- New **plugin hooks**, changed hook signatures, or removed hooks
- Changes to the **plugin loading** mechanism or `@opencode-ai/plugin` SDK
- New **tool capabilities** or changed tool argument shapes
- Changes to **permission** handling or approval flows
- Changes to **session management** (`--continue`, `--session`, `--model` flags)

### 2. Fetch detailed diffs (self-serve)

For deeper analysis, fetch diffs from the OpenCode repo using the FROM_VERSION and TO_VERSION values from the prompt header:

```bash
# Compare two tags to see all file changes
gh api repos/anomalyco/opencode/compare/{FROM_VERSION}...{TO_VERSION} --jq '.files[] | "\(.filename) (\(.status))"'

# Read a specific file at a given tag
gh api repos/anomalyco/opencode/contents/pkg/plugin/plugin.go?ref={TO_VERSION} --jq '.content' | base64 -d
```

Focus on files under `pkg/plugin/` (plugin system), `pkg/tui/` (TUI changes), and `pkg/client/` (SDK/API surface).

### 3. Check SDK updates

Check if a new `@opencode-ai/plugin` version is available:

```bash
npm view @opencode-ai/plugin versions --json
```

Cross-reference with what the release notes mention. If a relevant update exists, bump the version in the user's `~/.config/opencode/package.json` guidance or update our bridge plugin accordingly.

### 4. Scan the Canopy codebase

**Known integration points** (start here):

- `resources/opencode-canopy-bridge.ts` — bridge plugin that runs inside OpenCode, forwarding events to Canopy's hook server
- `src/main/agents/adapters/opencode.ts` — Canopy-side adapter (event normalization, CLI args, env vars, notch status)
- `src/renderer/src/components/preferences/OpenCodePrefs.svelte` — preferences UI
- `src/renderer/src/components/agents/OpenCodeExtras.svelte` — inspector extras
- `src/renderer/src/lib/agents/agentState.svelte.ts` — agent state handling (todo sync, question detection)
- `src/main/agents/types.ts` — shared agent types

**Then discover more** — search broadly for additional references:

```bash
grep -r "opencode" --include="*.ts" --include="*.yml" --include="*.md" --include="*.json" --include="*.svelte" -l .
grep -r "OPENCODE" --include="*.ts" --include="*.yml" --include="*.json" -l .
```

### 5. Apply changes

Be **proactive** — not just compatibility fixes but also:

- Adopt new OpenCode plugin hooks that provide richer data for the inspector
- Update event mappings if hook signatures or event types change
- Update CLI args if new flags are available and useful (new models, permission modes)
- Update the bridge plugin if the `Hooks` interface changes
- Update `OpenCodePrefs.svelte` if new configurable options become available
- Bump `@opencode-ai/plugin` types if the SDK changes

For each change, make a **targeted, minimal edit**. Do not reformat or restructure code beyond what the change requires.

### 6. Create or update the PR

Use the FROM_VERSION, TO_VERSION, and EXISTING_PR values from the prompt header.

**If no existing PR** (EXISTING_PR is empty):

1. Create the branch: `git checkout -b chore/opencode-compat`
2. Commit changes with descriptive messages (one commit per logical change group, use `chore:` or `fix:` prefix)
3. Push: `git push origin chore/opencode-compat`
4. Create PR targeting `next` with this structure:

```
Title: chore(deps): opencode compatibility update ({FROM_VERSION} → {TO_VERSION})

Body:
## OpenCode Compatibility Update

### Versions analyzed
[List each version analyzed]

### Relevant changes
[For each version: key changes that affected our codebase]

### Modifications made
[For each file changed: what was modified and why]

### SDK changes
[SDK version bump details, or "No SDK changes needed"]

### Risk assessment
[Low/Medium/High — explain any risks or breaking changes]
```

**If existing PR** (EXISTING_PR is a PR number):

1. Checkout the existing branch: `git fetch origin chore/opencode-compat && git checkout chore/opencode-compat`
2. Commit incremental changes
3. Push: `git push origin chore/opencode-compat`
4. Update the PR title and description to cover the expanded version range using `gh pr edit`

### 7. If no changes needed

If after analysis you determine no code changes are required:

1. Do NOT create a branch or PR
2. Write a summary to `$GITHUB_STEP_SUMMARY`:

```bash
cat >> "$GITHUB_STEP_SUMMARY" <<'EOF'
## OpenCode Compatibility Check

Analyzed versions: {FROM_VERSION} → {TO_VERSION}

**No code changes needed.** The release changes do not affect Canopy's integration.
EOF
```

## Tone

Be precise and factual. State what you found, what you changed, and why. No filler or commentary beyond what is needed to explain each decision.
