# Desktop remote UI

This module owns the device-side views for another computer; `remote::client` owns
the connection, catalogs and command journals. Source behavior is not GUI or native
acceptance evidence.

## Surfaces

- `mod.rs` — `RemoteWindow` state, operation runner (`run` / `run_in`), result
  dispatch (`Update`), host selection, layout persist/restore and the modal owners.
- `shell.rs` — window chrome: Preferences-style navigation (computers with a status
  dot and state text, workspace pages, computer pages), page header with
  connection actions, offline/connecting status and page-level notices. The
  embedded workspace replaces the navigation with a tab strip; the tool-panel
  variant (`RemoteWorkspace::tools`) renders only the page content.
- `chrome.rs` — presentation builders shared by the pages (header, navigation rows,
  list rows, loading/done/failure notices, empty states, segmented control).
- `hosts.rs` — Add computer (pairing, SSH key check, advanced address/port) and the
  selected computer's Connection page (status, routes from `routes.rs`, local name).
- `inline.rs` — `ConnectionSetup`: the same pairing form or one computer's routes and
  local name, rendered inline in Preferences → Connections by a `RemoteWindow` in
  inline mode. It emits `ConnectionSetupEvent::{Paired, Closed}`; closing cancels a
  waiting pairing request.
- `catalog.rs` — Projects: checkouts, chats (opened as normal ChatView tabs),
  terminals, opening a project and starting a terminal.
- `discovery.rs` — New chat: destination, READ-scoped agent/account discovery and
  chat creation. Discovery never takes the mutation gate.
- `workspace.rs` — Files (folder rows in file-tree geometry, search, create via a
  name prompt, editor with save/discard/preview) and Changes (status letters and file
  rows like the local Changes panel, diff pane, commit, Pull/Push, history, refs and
  worktree actions).
- `notes.rs` — process-local scratchpad per host and checkout, Markdown preview and
  export to a named new file.
- `settings.rs`, `settings/` — host settings: preferences, agents, profiles,
  integrations, write-only secrets, authorized devices and diagnostics.
- `preview.rs` — attachment, text/Markdown and isolated HTML preview windows.
- `lifecycle.rs` — project close preflight and risk summary.

## Interaction rules

`run_in(scope, …)` ties an operation to one control: that control shows the loading
state (`pending`) and its failure or result appears next to it (`notice(scope)`).
`run` keeps the page-level notice for owners outside this module (routes, terminal).
Navigation clears page messages. Confirmations use `components::modal::Confirmation`
with a named verb; destructive results use danger styling (`confirm_action`). Names
are asked with `TextPrompt`. Developer details (edit markers, remote/upstream,
worktrees, uncertain operations, device scopes) stay behind `Disclosure` sections.

Remote chats always open in the main window's ChatView. A persisted `Page::Chat`
layout from older builds only hands that chat to a ChatView tab.

No render starts I/O; async results check the original generation. Tests:
`cargo test --locked --bin canopy-desktop remote_client`.
