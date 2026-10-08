# Files and editor

The Files sidebar reads the active worktree (or a folder without Git). It initially
lists only the root's direct children; expanding a directory loads one more level.
Directories come first, then files; single click or Enter opens a file. Refresh
rereads only the currently expanded levels, and
New file creates a named file relative to this worktree using create_new. Parent
directories must already exist; existing files are never overwritten by creation.

Cmd+P opens a local file search with arrow navigation and Enter; queries run off
the UI thread. It walks non-ignored paths on demand, independent of tree expansion,
with a cancellation token and short debounce. The first 100 matching paths are shown.
Cmd+S on macOS or Ctrl+S on Windows saves the focused editor. Opening an
already-open path in a workspace selects its existing pane.

## Ownership

- `files.rs` / `files/listing.rs`: bounded one-level directory reads, path validation,
  text decoding and optimistic atomic writes. Directory reads and Git ignore checks
  run on the existing Git worker. `file_decorations` is a separate status request;
  it does not block publishing the initial file listing. No Git subprocesses.
- `AppState.files`: one active-root read loop with a cache of expanded levels,
  coalesced desired-directory requests, cancellation and a separate Git decoration
  snapshot. Collapsed directory contents are evicted. Changed-directory events
  invalidate only the affected cached levels; index/ignore/rescan events invalidate
  the displayed tree. Late reads are rejected before publishing.
- `files/tree_watch.rs`: scoped native event handling. macOS uses one FSEvents root
  stream (plus external Git metadata when needed); registration does not enumerate
  descendants. The callback ignores unopened dependency contents and Git objects.
  Other backends register nonrecursive listed/tracked directories. Tracked-parent
  metadata comes from the Git index so closed folders still receive Git decoration
  updates. Untracked folders remain opaque; ignored dependency changes do not trigger
  Git status reads. Watchers and queued reads stop on root change or quit.
- `files/search.rs`: cancellable on-demand Cmd+P walk using the already-locked
  `ignore` 0.4.33 matcher, including `.gitignore` in folders without Git. Searches
  do not populate the file tree or run when a project merely opens.
- `FileTree`: presentation, local selection and expansion; replacing data in the
  same root preserves expansion. Expanded-folder requests and file-open events are
  separate. `file_nodes.rs` builds only the cached hierarchy. Loading/error states
  and ignored flags belong to rows; metadata updates preserve in-flight motion.
  Async children animate into place without losing or reordering sibling rows.
- `AppState.editors`: PaneId → editor view, dirty-state notifications and shared
  Save / Discard / Cancel protection for tab/pane/project/window/application close.
- `EditorView`: GPUI Kit EditorState, document baseline, dirty buffer, watchers,
  asynchronous load/save, conflict banner and Reload confirmation.

Editor changes only notify the workspace when dirty/saving state changes. The
input engine owns ordinary typing, selection and undo. No processes are started
for editor panes. Hidden editor buffers stay alive through tab/worktree switches.
Closing a pane removes its view and watchers. Save operations are protected by
close guards, and quit waits for pending file creation before the final snapshot.

## Save and external changes

Files are UTF-8 text, at most 2 MiB and 50,000 lines. UTF-8 BOM and CRLF convention
are retained; executable permissions survive save. Save writes a same-directory
temporary file and replaces the original only after checking its bytes against
the loaded baseline. Files deleted externally are not silently recreated.
Windows publication uses `MoveFileExW` with replace and write-through flags; an
open handle without delete sharing or a read-only/ACL failure leaves the original
unchanged and the buffer dirty for retry. No delete-before-rename fallback exists.

When an agent changes a clean document, it reloads. A dirty document keeps its
buffer and shows a conflict; save cannot overwrite the disk version. Reload asks
before discarding edits. Copy edits elsewhere before Reload if both versions are
needed. There is no merge editor or force-overwrite action in this milestone.
Atomic replacement is not a filesystem-wide lock against arbitrary external
writers in the final check/rename interval.

The tab title marks unsaved changes. The editor has no permanent toolbar; its
contextual Cmd+S/Ctrl+S action saves only the focused editor, while terminal
Ctrl+S remains PTY byte 0x13. The editor tab context menu offers Reload file from disk, and errors show
a contextual Reload action. Close offers Save and close,
Discard and close, or Cancel. A failed/conflicting save leaves the document open.
Worktree deletion is blocked while its editor has unsaved edits or a pending save.

## Restore and bounds

SQLite already persists editor kind, cwd, relative resource path, tab order,
splits and focused PaneId. Restore lazily loads editor panes in the active tab;
other tabs load when selected. No editor text/undo history is stored in the
workspace snapshot, so force-killing the application can lose unsaved edits.

The file tree excludes `.git`, symlinks and non-regular files. Git-ignored names,
including `.env` and `node_modules`, stay visible in a muted color. Leading dots alone
do not cause muting, and tracked entries are not classified as ignored merely because
a rule matches. Ignored directories are entered only on expansion, one level at a time.
Non-UTF-8 filenames are omitted with a warning. Symlinks are deliberately not
followed when reading/writing, including symlinked intermediate directories.
Windows junctions and other reparse points are rejected by file attributes as
well. Existing components are canonicalized and must remain under the canonical
root; path casing is never globally folded.
The former global 50,000-entry recursive index is gone. Safety bounds now apply to
50,000 direct children per directory, 100,000 entries in expanded levels and 512
open directories / 64 levels. Warnings are attached to the affected directory, not
unopened descendants. Cmd+P limits results to 100 and visits to 200,000, reporting
partial searches instead of presenting them as complete. It still does not search
file contents.
This milestone does not implement rename/delete, full-text repository search,
LSP, formatting, or a filesystem change merge UI.

Syntax highlighting uses the optional Tree-sitter features of the existing pinned
GPUI Kit 0.7.1 facade: Rust, TypeScript, JavaScript, JSON, Python, HTML, CSS,
Markdown, TOML, YAML, Bash and Svelte. Other extensions remain plain text. Cargo.lock
includes those parser dependencies.

## Verification of this milestone

A disposable Canopy data directory and Git repository were used for GUI checks:
file tree expansion/open, Cmd+P, editing with Cmd+S, dirty markers, Cancel and
Save and close, external-write conflict preserving edits, confirmed Reload,
New file, syntax highlighting and editor-tab restore after quit/relaunch.
The new file was then opened as a diff, staged, and committed through Changes
(test commit `c554c22`, only in the disposable repository).

Filesystem tests cover ignored files, nested paths, binary/size rejection,
symlink boundaries, BOM/CRLF, executable permissions, optimistic conflicts,
new-file collision, editor metadata restore and the shared index worker. GPUI
tests render the real EditorView and cover Windows Ctrl+S success, conflict,
focus isolation between editors and terminal focus.
The full regular Cargo test suite passed; opt-in live-provider tests remain ignored.

The final hidden-tab callback and watcher-race fixes were compiled/tested but
not replayed in GUI: macOS locked during the final pass. They bind callbacks to
the owning window rather than requiring the editor entity to be currently rendered.
No claim of measured 120 FPS or qualification of very large repositories is made.

Files has no independent scroll area or row-count height cap. Expanded rows take
their full height inside the sidebar, which owns the only vertical ScrollHandle.
FileTree culls rows against that shared viewport and uses its offset for keyboard
navigation. A geometry probe updates the viewport only when it changes; it does
not request frames while the sidebar is stationary.

Shared sidebar scrolling was checked in the release GUI with 100 files: Projects
and Files headers scroll away together, and Tools appears after the last file.
The final keyboard-navigation check was interrupted by user interaction and is
not counted as verified.

The sidebar scrollbar is a sibling overlay of the scrolling content, anchored to
an explicit relative viewport. It must not be attached inside the scrolling
column. Release GUI verified top/bottom handle position with 100 files; the footer
remains outside the track. The drag attempt did not provide a confirmed result.

ThemeColor and ThemeTokens are synchronized after Canopy overrides, so native
Cmd+F/replace panels use the same surfaces, borders and button states. GUI checked
opening Cmd+F, typing a query and highlighted match/counter. The closing check
was interrupted by user interaction.

Files decorates tracked modifications (M/yellow), new or untracked files (A/green),
renames (R/blue) and conflicts (U/red), including staged changes. Existing parent
folders aggregate descendants: identical states retain their color, mixed changes
use modified/yellow and conflicts take precedence. Deleted files do not create
fake openable tree rows; their existing parents still reflect the deletion.
Status is scoped to the selected root, including subdirectories and linked worktrees.
Index, HEAD, refs/config and common Git-directory events refresh the decorations;
object writes and lock-file churn do not trigger scans. No idle polling is added.

## Images

Files and Cmd+P route PNG, JPEG, WebP, GIF, BMP, ICO, TIFF and SVG to a read-only
ImagePreview, fitted to the pane without distortion. New tabs persist PaneKind::Image,
cwd and resource; older editor tabs pointing at an image are also routed to the
preview at restore. They never start a terminal or allocate a text-editor input.
The existing native GPUI decoder handles image loading, SVG and GIF rendering.
Encoded files are limited to 20 MiB; unreadable/corrupt formats show an error.
Native file watching refreshes changed bytes, and removed panes drop their tasks
and release image assets. This is a preview, not image editing or video playback.
Release GUI confirmed PNG and SVG display from Files with contain sizing. The
live refresh check was interrupted by a tab change and is not counted as verified.
Cmd+P queues Enter while its new query is still running, rather than opening a
stale previous result.

## Font and video previews

TTF, OTF, WOFF and WOFF2 open as read-only font specimens. Wuff decodes web fonts;
ttf-parser extracts the exact glyph outlines into a local SVG specimen (several
sizes, alphabet, digits, Polish text and a sample of available characters). Fonts
are not installed or registered globally; same-family subsets cannot replace one
another. Missing sample characters are explicitly reported, without system-font
fallback. Variable fonts use their default axes; bitmap/color-only glyphs are not
rendered as outlines. Compressed input is capped at 16 MiB and the WOFF header's
expanded size at 32 MiB. No font editor or axis controls are provided.

MP4/M4V/MOV and MKV route to a basic macOS AVFoundation preview. MP4-family support
uses system codecs; MKV is attempted and reports an unsupported-media message if
the platform cannot play it. No FFmpeg, codecs bundle or transcoding is included.
The player starts paused and has play/pause, a seek bar, ±10 seconds and time.

`native/video_preview.m` owns AVPlayer and AVPlayerLayer; `video_native.rs` is a
main-thread-only RAII bridge. `video_preview.rs` supplies GPUI controls and samples
time only during loading/playback/seek. The native layer is sized from the pane's
canvas, with implicit Core Animation movement disabled. Hiding a pane or opening
a Canopy modal hides the layer and pauses playback; closing the pane releases it.
There is no background playback, playlist, audio-file viewer or new media library.
The first-frame native layer and MP4 duration were observed in the GUI; interactive
play/seek checks were interrupted by window targeting and remain unqualified.

PaneKind::Font/Video with cwd/resource persists through the same workspace snapshot.
Legacy Editor tabs with these extensions are routed to previews. Decoder/player
handles, playback time and font data never enter SQLite. Restored videos start paused.

Dependencies: wuff 0.2.9 for WOFF decompression and the already-resolved ttf-parser
0.25.1 for outlines. A small Objective-C bridge is compiled by cc 1.4.5 and links
system AppKit, QuartzCore, AVFoundation and CoreMedia. GPUI versions stay pinned.
References: https://docs.rs/wuff/0.2.9/wuff/ and https://developer.apple.com/av-foundation/ .

## Lazy tree verification (2026-09-10)

Regressions cover shallow initial reads, ignored directory entry/expansion, normal
dotfiles and tracked ignore-rule matches, path/symlink boundaries, cancellation,
per-directory bounds, on-demand Quick Open across unopened directories, opaque
untracked decorations, scoped watcher events and async row insertions/motion.
Only loaded levels contribute to the file-tree entry count. Git's own tracked index
metadata is a separate concern and is still read to supply correct decorations.

Release GUI was checked in a disposable `Canopy Files Test.app` with a separate
CANOPY_DATA_DIR and Git fixture: shallow root display; muted `.env`/`node_modules`;
expanding dependency folders one level at a time; opening ignored `.env`; Cmd+P
finding `src/deep/main.rs` while `src` stayed closed; live decoration of closed `src`
after a file modification; expanding 1,200 children and scrolling to both boundaries;
collapsing the large branch without losing siblings; live added-file and `.gitignore`
updates; normal quit and process cleanup. No test terminal/agent was launched.
The initial clipboard paste into macOS Open dialog timed out; a fresh state and its
native text-field setter completed the fixture selection. No performance/FPS claim
or other-platform GUI qualification is inferred from these checks.

The full Cargo suite passed with `--test-threads=1`; formatting, Clippy with warnings
as errors, and the macOS release build passed. The test fixture directory path was
recorded in `/tmp/canopy-lazy-files-path`; it is separate from the user's workspace/database.
