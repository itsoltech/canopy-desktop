# Claude Code Compatibility Analysis

You are analyzing new Claude Code CLI releases to determine whether the **Canopy** desktop application (Electron + Svelte 5) needs code changes to stay compatible or to adopt new features.

## Context

The workflow has provided these values at the top of the prompt:

- **FROM_VERSION** — the last version we checked (exclusive lower bound)
- **TO_VERSION** — the latest available release (inclusive upper bound)
- **EXISTING_PR** — PR number if a compatibility PR is already open (empty if none)
- **REPO** — this repository (owner/repo format)

The **changelog repo** is `marckrenn/claude-code-changelog` — an unofficial archive of Claude Code system prompts, flags, and metadata.

The **target branch** for PRs is `chore/claude-code-compat`.

Release notes for each new version are appended below under `## Release Notes`.

## Your task

### 1. Understand the releases

Read the release notes provided below. For each version, identify:

- New or removed **feature flags**
- Changed or removed **environment variables** and **config keys**
- New **hooks**, **permission models**, or **tool capabilities**
- **System prompt changes** that affect how Claude Code behaves
- **Bug fixes** that may affect how we invoke Claude Code
- **SDK changes** related to `@anthropic-ai/claude-agent-sdk`

### 2. Fetch detailed diffs (self-serve)

For deeper analysis, fetch diffs from the changelog repo yourself using the FROM_VERSION and TO_VERSION values from the prompt header:

```bash
# Compare two tags to see all file changes
gh api repos/marckrenn/claude-code-changelog/compare/{FROM_VERSION}...{TO_VERSION} --jq '.files[] | "\(.filename) (\(.status))"'

# Read a specific file at a given tag
gh api repos/marckrenn/claude-code-changelog/contents/meta/flags.md?ref={TO_VERSION} --jq '.content' | base64 -d

# Read metadata
gh api repos/marckrenn/claude-code-changelog/contents/meta/metadata.md?ref={TO_VERSION} --jq '.content' | base64 -d
```

Focus on files under `meta/` (flags, metadata, CLI surface) and notable system prompt changes.

### 3. Check SDK updates

Check if a new `@anthropic-ai/claude-agent-sdk` version is available:

```bash
npm view @anthropic-ai/claude-agent-sdk versions --json
```

Cross-reference with what the changelog mentions. If a relevant update exists, bump the version in `package.json`.

### 4. Scan the Canopy codebase

**Known integration points** (start here):

- `.github/workflows/` — Claude Code action configurations (`anthropics/claude-code-action@v1`), model args, allowed tools
- `.github/prompts/` — prompt templates passed to Claude Code action
- `.claude/` — harness settings (`settings.json`), skills
- `src/main/changelog/` — changelog fetching module
- `CLAUDE.md`, `AGENTS.md` — agent instruction files

**Then discover more** — search broadly for additional references:

```bash
grep -r "claude" --include="*.ts" --include="*.yml" --include="*.md" --include="*.json" -l .
grep -r "anthropic" --include="*.ts" --include="*.yml" --include="*.json" -l .
grep -r "claude-code" -l .
```

### 5. Apply changes

Be **proactive** — not just compatibility fixes but also:

- Adopt new Claude Code features that benefit our workflows (new hooks, better permission models, improved tool specs)
- Update `claude_args` if new CLI flags are available and useful
- Update prompts if system prompt behavior changes affect our instructions
- Update `CLAUDE.md` or `AGENTS.md` if new conventions or capabilities are relevant
- Bump SDK version if appropriate

For each change, make a **targeted, minimal edit**. Do not reformat or restructure code beyond what the change requires.

### 6. Create or update the PR

Use the FROM_VERSION, TO_VERSION, and EXISTING_PR values from the prompt header.

**If no existing PR** (EXISTING_PR is empty):

1. Create the branch: `git checkout -b chore/claude-code-compat`
2. Commit changes with descriptive messages (one commit per logical change group, use `chore:` or `fix:` prefix)
3. Push: `git push origin chore/claude-code-compat`
4. Create PR targeting `next` with this structure:

```
Title: chore(deps): claude code compatibility update ({FROM_VERSION} → {TO_VERSION})

Body:
## Claude Code Compatibility Update

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

1. Checkout the existing branch: `git fetch origin chore/claude-code-compat && git checkout chore/claude-code-compat`
2. Commit incremental changes
3. Push: `git push origin chore/claude-code-compat`
4. Update the PR title and description to cover the expanded version range using `gh pr edit`

### 7. If no changes needed

If after analysis you determine no code changes are required:

1. Do NOT create a branch or PR
2. Write a summary to `$GITHUB_STEP_SUMMARY`:

```bash
cat >> "$GITHUB_STEP_SUMMARY" <<'EOF'
## Claude Code Compatibility Check

Analyzed versions: {FROM_VERSION} → {TO_VERSION}

**No code changes needed.** The release changes do not affect Canopy's integration.
EOF
```

## Tone

Be precise and factual. State what you found, what you changed, and why. No filler or commentary beyond what is needed to explain each decision.
