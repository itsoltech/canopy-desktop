# Desktop remote UI

This module owns device views and controls; `remote::client` owns connection and
command journals. Source behavior is not GUI or native acceptance evidence.

The catalog discovers agents and explicit accounts with READ-scoped `ListAgents`.
Composer commands, skills and model choices come from the host/account catalog;
its refresh does not acquire the Send gate. File mentions use workspace search,
and PR choices retain the host's context ID for host-side expansion.

Activity cards use semantic kind/state, plan entries, typed files, bounded diff
previews, background status and parent call identity. A full diff opens in a
virtualized reader. Thread selection is scoped to the history epoch and retains
the transcript list owner; search jumps also select a matched nested thread.

Fork and Fork with draft select a destination workspace or Private chats. Edit is
a separate sent-user-message workflow: PreviewEdit supplies the original context
and images, the previous draft is retained for Cancel, file restore defaults off,
and a restored edit must refresh its preview. Edit, fork and explicit Shell mode
use durable `ChatMutation` requests. Reconcile queries/retries the original ID;
uncertain effects never replay automatically. Normal Message mode sends `!`
literally. Shell Stop targets the displayed running item and run.

Project Close calls PreflightClose and reviews runtime, queue, observer and dirty
risks before CloseResource. Host errors remain visible. Project removal and cleanup
semantics are host-owned; a partial cleanup failure is not presented as success.
Closing workspaces retain a Retry cleanup action and cannot be selected for new
chats or forks.

Workspace media uses signatures before extensions. Images use the shared image
view; fonts use the existing font specimen loader; native macOS attachments use
Quick Look. Markdown and diffs have app-owned readers. HTML uses authenticated
ReadChatPage bytes, or the selected workspace editor buffer, inside a sandboxed
iframe with no file URLs, credentials or Canopy RPC. The existing native page
renderer currently requires macOS; other platforms report that limitation.

Notes follow the existing scratchpad contract: process memory scoped by host and
workspace, editable Markdown, with explicit export to a chosen new workspace
file. Notes do not imply host synchronization or persistence across process exit.

All new controls use existing components and theme tokens. No render starts I/O;
async results check the original generation. New pure regressions cover media
classification and epoch-bound thread navigation. GUI, native transport, live
providers, real vault, and platform acceptance are NOT RUN.

R14 verification checkpoint: pinned rustfmt completed. Targeted
`cargo test --locked --bin canopy-desktop ui::remote_client::` reached binary
compilation but could not run tests while host Edit/Shell, fork bridge and project
cleanup changes were in flight (log `/var/tmp/r14-tests4.log`). That attempt
reported no errors in this module; this is not a passing build or test result.
Rerun the targeted tests after host integration settles.
