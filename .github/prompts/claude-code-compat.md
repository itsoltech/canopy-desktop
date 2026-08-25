# Claude Code Compatibility Analysis

## ITSOL Powers requirements

Before any setup or analysis step, invoke `itsolpowers:using-itsolpowers` with the `Skill` tool. Use `itsolpowers:itsol-current-tech-context` for version and documentation checks, then use the smallest relevant implementation or review skills for any workflow, prompt, SDK, or app-code changes. If the diff spans independent areas, delegate focused review/analysis to `itsolpowers:*-review` subagents with `Agent` or `Task`.

Do not fabricate approval for any ITSOL plan or gate. If the selected ITSOL workflow requires human approval before a change, do not create or update a PR; write the blocker to `$GITHUB_STEP_SUMMARY` using the normal no-change path.

Do not mention ITSOL Powers, skills, subagents, routing, or tool usage in PR bodies, commit messages, commit bodies, or step summaries.

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

> **Known blocker — these commands are currently denied.** The workflow allowlists
> `Bash(gh api repos/marckrenn/claude-code-changelog/:*)`. That prefix ends mid-token, so no real
> command matches it and every `gh api` call against the changelog repo is denied — `compare/`,
> `contents/` and `releases/` alike, with or without `--jq`, quoted or bare. Removing shell pipes
> does **not** help; that was an earlier misdiagnosis. `Bash(npm view:*)` and `Bash(gh pr view:*)`
> are allowed in the same session because their prefixes end on a token boundary.
>
> Probe once with the `compare` call below. If it is denied, **stop probing** and use `WebFetch`
> instead — see the next block. Do not represent release-notes-only analysis as a full diff review.
>
> The fix is one token in `.github/workflows/claude-code-compat.yml`
> (`Bash(gh api repos/marckrenn/claude-code-changelog/:*)` → `Bash(gh api:*)`), but this job cannot
> apply it: `RELEASE_TOKEN` has no `workflow` scope, so pushing a `.github/workflows/` change is
> rejected. It needs a maintainer.

> **Use `WebFetch` — it works, despite not being listed in `--allowedTools`.** An earlier note here
> claimed the raw-content fallback was denied too. That was wrong, and it cost twelve runs a
> recoverable coverage gap. Verified working on the v2.1.241 → v2.1.245 run:
>
> - `https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md` — the **official**
>   changelog. This is the important one: it recovers the entries the release notes drop behind
>   "… +N more CLI changelog entries". Fetch it before concluding a release is uneventful.
> - `https://raw.githubusercontent.com/marckrenn/claude-code-changelog/{TAG}/meta/metadata.md` —
>   absolute prompt-token totals and the system/tools split, not just the deltas.
> - `https://raw.githubusercontent.com/marckrenn/claude-code-changelog/{TAG}/meta/prompt-stats.md` —
>   per-file prompt inventory. Names individual tool-description files, which is how a change in the
>   tools token share can be attributed instead of guessed at.
> - `https://raw.githubusercontent.com/marckrenn/claude-code-changelog/{TAG}/meta/flags.md` —
>   internal feature flags (not CLI arguments, despite the heading).
>
> Two routes that do **not** work, so don't spend turns on them: the `api.github.com/.../compare/`
> JSON is truncated before its `files` array (you get the commit list only), and the rendered
> `github.com/.../releases/tag/{TAG}` page mostly fails to load.
>
> `WebFetch` answers through a small model with a ~125-character quoting limit, so it will refuse
> "return this verbatim" and summarize instead. Ask narrow, specific questions and issue several
> fetches rather than one broad one.

```bash
# Compare two tags to see all file changes
gh api repos/marckrenn/claude-code-changelog/compare/{FROM_VERSION}...{TO_VERSION} --jq '.files[].filename'

# Read a specific file at a given tag
gh api repos/marckrenn/claude-code-changelog/contents/meta/flags.md?ref={TO_VERSION} -H "Accept: application/vnd.github.raw"

# Read metadata
gh api repos/marckrenn/claude-code-changelog/contents/meta/metadata.md?ref={TO_VERSION} -H "Accept: application/vnd.github.raw"
```

Focus on files under `meta/` (flags, metadata, CLI surface) and notable system prompt changes.

### 3. Check SDK updates

Check if a new `@anthropic-ai/claude-agent-sdk` version is available:

```bash
npm view @anthropic-ai/claude-agent-sdk versions --json
```

Cross-reference with what the changelog mentions. If a relevant update exists, bump the version in `package.json`.

`package.json` and `package-lock.json` must move in the same commit — CI runs `npm ci`, which
fails when the two disagree. Never edit the dependency range on its own.

Prefer `npm install`, which regenerates the lockfile properly:

```bash
npm install @anthropic-ai/claude-agent-sdk@{VERSION} --save-exact=false
```

> **Currently denied.** `Bash(npm install:*)` is not in this workflow's allowed tools (only
> `Bash(npm view:*)` is), so the command above fails. Until a maintainer adds it, hand-edit both
> files together:
>
> 1. Confirm the release has the same dependency shape as the one being replaced, so no lockfile
>    node needs adding or removing:
>    `npm view @anthropic-ai/claude-agent-sdk@{VERSION} engines peerDependencies optionalDependencies`
> 2. Update `version`, `resolved` and `integrity` for the main package and each optional platform
>    package in `package-lock.json`, plus both dependency ranges (`package.json` and the lockfile's
>    root `packages[""]` entry).
> 3. Read every `integrity` value from the registry with
>    `npm view <package>@{VERSION} dist.integrity`. Never derive, compute or guess a hash.
> 4. Verify the result parses (`jq '.packages' package-lock.json`) and that the diff touches only
>    `version`, `resolved` and `integrity`.
>
> State in the PR body that the lockfile was hand-edited and that CI's `npm ci` is the real check.

### 4. Scan the Canopy codebase

**Known integration points** (start here):

- `.github/workflows/` — Claude Code action configurations (`anthropics/claude-code-action@v1`), model args, allowed tools
- `.github/prompts/` — prompt templates passed to Claude Code action
- `.claude/` — harness settings (`settings.json`), skills
- `src/main/changelog/` — changelog fetching module
- `CLAUDE.md`, `AGENTS.md` — agent instruction files

**Then discover more** — search broadly for additional references using the `Grep` tool (not `grep`
via Bash, which also walks `node_modules/` and buries real hits):

- pattern `claude`, glob `**/*.{ts,yml,md,json}`
- pattern `anthropic`, glob `**/*.{ts,yml,json}`
- pattern `claude-code`

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

> **Write the description to a `.txt` file and pass it with `gh pr edit --body-file`.** GitHub caps
> the body at 65,536 characters and each run must re-emit it in full, so the description is held
> flat by condensing the oldest per-release section on every increment. Do **not** stage it as
> `.md`: the repository's `PostToolUse` hook runs `npx prettier --write --ignore-unknown` on every
> file written, and prettier pads markdown table cells out to the column width — with impact cells
> this long that added ~18 kB of pure whitespace in one increment and pushed the body to 90% of the
> cap. `--ignore-unknown` skips `.txt`, so the tables stay compact. The scratch file cannot be
> deleted afterwards — `rm` and `sed -i` are both blocked — so leave it untracked and never `git add`
> it; stage only the files you actually changed.

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
