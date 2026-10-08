# New worktree from a task

`New worktree` in Tasks or task details opens the same one-step form as the
sidebar. The task suggests a branch name; Canopy owns the stable generated
directory path. Collapsed `Options` contains `No agent` (the default) and the
configured Claude/Codex profiles. No agent creates an empty workspace.

With an agent selected, Canopy fetches the current task and every comment page,
then downloads attachments before creating the worktree. Jira and YouTrack use
their existing authenticated download adapters. GitHub recognizes uploaded asset
links in Markdown and HTML, including comments. Only requests to the exact GitHub attachment origin carry the account token;
CDN redirects do not. Unsupported redirects and private assets requiring browser login
produce an error instead of silently omitting the file.

The agent starts directly in the new worktree using the selected profile. The
task context is **not** passed as a CLI prompt or followed by Enter. For every
tool, Canopy inserts the draft as soon as its PTY has started, without waiting
for a hook, terminal mode announcement or visibility. Multiline text is always
wrapped in bracketed-paste delimiters; no submit key is appended. This immediate
insertion does not confirm that the tool has finished login or onboarding.
`Paste task prompt` retries insertion if the PTY has not accepted it yet.
The draft is kept in pane metadata until the PTY accepts the paste; it is then
cleared, so later output, restart and resume do not paste it again. Unsent text
already in the provider's composer is not backed up by Canopy.

Files use private `task-context/task-*` directories beneath Canopy's data directory
(or `CANOPY_DATA_DIR` for tests), with sanitized, numbered names. Absolute `@`
paths use the selected Claude/Codex reference grammar; Windows backslashes and
spaces are quoted without shell parsing. An incomplete operation removes its bundle;
a successful handoff retains files for provider-session resume. Retained bundles
currently have no automatic pruning and are not deleted when closing a tab.
On Windows the private root has a protected owner/System DACL, and filenames also
reject reserved device names, forbidden characters and trailing dots or spaces.

Limits: 512 KiB prompt, 100 comment continuation pages, 64 attachments, 25 MiB per
file and 100 MiB total. Reader truncation boundaries and download failures fail
the handoff before worktree creation. A failure after creation reports that the
worktree exists; the dialog cannot create it a second time.

Verification: focused task-context transport tests cover multi-page comments,
comment attachments, credential separation and incomplete-bundle cleanup. Session
tests cover SQLite restore with a pending draft. Live tracker/provider end-to-end
qualification is separate from these controlled checks.

On macOS, an isolated `CANOPY_DATA_DIR` GUI run verified both creation choices:
without an agent the saved workspace had zero tabs; selecting the controlled
Codex executable created one terminal in the new worktree. A restored pending
draft sent zero bytes before bracketed mode was enabled, then exactly one
64-byte bracketed paste with no trailing Return. The persisted pending draft
was cleared. The original controlled process implements the session hook handshake;
this does not qualify live Claude/Codex login, trust dialogs or tracker accounts.

The immediate-paste policy supersedes the historical readiness-gated GUI check
above. It is covered by unit tests only; no new E2E run was performed.
