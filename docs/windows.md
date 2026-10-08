# Windows support

This is the implementation and qualification checklist for the Windows port.
The first supported target is Windows 11 x64 with the MSVC Rust target in a
local desktop session. A checked source row or a successful macOS build does
not count as native Windows qualification.

## Status matrix

| Capability | Implemented | Unit/static | Native Windows | Native macOS | Limitation |
| --- | --- | --- | --- | --- | --- |
| Full Cargo graph and optional features | Partial | CI definition | Pending | Check, Clippy and build | First native run stopped in a nonessential `cl` diagnostic before Cargo; the corrected gate has not run yet. |
| Agent relay protocol and token routing | Yes | Unit and integration tests | Pending | Automated | Provider live sessions remain a separate qualification. |
| Agent relay transport | Yes | UDS exercised locally; Win32 module and tests cross-checked | Pending | UDS round trip | Windows named-pipe execution, DACL and cancellation timing need a native run. |
| Agent hook helper and command | Yes | Config, packaged-helper and native-only command test definitions | Pending | Automated helper round trip | Live Codex/Claude interpreters still need native provider checks. |
| Shell environment and executable resolution | Yes | Unit tests and MSVC Clippy harness | Pending | Existing behavior | PowerShell profiles are intentionally not executed; native CLI shim qualification is pending. |
| ConPTY and process-tree cleanup | Yes | Vendored patch and MSVC Clippy harness | Pending | Existing behavior | Native Job Object, resize, argv and final-output tests are defined but not executed yet. |
| Application directories and private profiles | Yes | Unit tests and MSVC platform harness | Pending | Automated | Gemini auth hardlinks require the data directory and user profile on one volume. |
| Secure credentials | Yes | Validation and MSVC platform harness | Pending | Existing Keychain behavior | Native Credential Manager round trip and ACL inspection remain pending. |
| Git hooks, signing and authentication | Yes | Existing Git tests and MSVC focused harness | Partial | Automated | SSH Pull with a default RSA key is verified; hooks, signer UI, GCM and agent-only handshakes remain pending. |
| Notch | Yes | Pure geometry tests and MSVC focused harness | Pending | Existing behavior | Real click-through, focus, DPI, fullscreen and multi-monitor behavior need native GUI qualification. |
| Window chrome, shortcuts, fonts and accessibility | Yes | Unit tests, macOS graph and focused Win32 harness | Pending | Existing behavior | Caption/Snap, AltGr/IME, accessibility and system motion changes need native GUI qualification. |
| Files, watchers and worktree safety | Yes | Unit/integration tests and Win32 API source check | Pending | Existing behavior | Native NTFS junction, case-sensitive directory, UNC and sharing-violation execution remain pending. |
| Video and attachment previews | No | Pending | Pending | Existing behavior | Windows native preview adapters are W8. |
| Packaging and installer | No | Pending | Pending | macOS bundle only | Windows artifacts, signing and clean-machine checks are W9. |

## Native build environment

The repository pins Rust 1.95.0 and `x86_64-pc-windows-msvc`. Development and
CI require Visual Studio 2022 Build Tools with Desktop development with C++, a
Windows 11 SDK, CMake, Perl and NASM. Those tools cover the current `cc`,
tree-sitter, bundled SQLite, vendored OpenSSL/libgit2/libssh2 and GPUI Windows
dependency graph. Keep `Cargo.lock`; dependency failures must be fixed in the
resolved graph instead of deleting it.

The GitHub Actions runner in `.github/workflows/windows.yml` is a native MSVC
compile and test gate on `windows-2022`. It is useful build evidence, while the
product qualification rows still require Windows 11 desktop hardware or a VM
with a real interactive session.

## Agent CLI contract

Agent profiles must resolve to native executables or explicit Node launchers.
WSL is outside the first support claim. Git Bash may be required only when a
specific provider documents that interpreter; Canopy must surface that missing
requirement instead of silently launching WSL or substituting a shell.

Canopy uses a Unix domain socket on Unix and a local Windows named pipe on
Windows. The endpoint kind and address are passed separately. The Windows pipe
rejects remote clients and uses a protected DACL for LocalSystem and the object
owner. Messages are bounded to 1 MiB, the accepted-event queue to 128 entries,
and the helper receives an acknowledgement only after token validation and
queue admission. Windows connect, read and write operations use overlapped I/O.
A persistent manual-reset stop event cancels both accept and an active exchange,
including the interval in which the server recreates its pipe instance. One
deadline covers the helper's connect, request and acknowledgement. After reading
the ACK, the helper sends a bounded receipt before the server disconnects; no
blocking `CallNamedPipeW` or `FlushFileBuffers` remains. The GUI uses the Windows
subsystem; the sibling `canopy-agent-hook.exe` remains a console helper so
providers can pipe JSON over stdin without changing the GUI process mode.
Canopy verifies that the sibling exists and reports the same hook protocol
version before launching an agent. Codex 0.154.0 selects `commandWindows`; that
command explicitly invokes the helper through encoded PowerShell instead of
depending on whether a quoted path is executed or echoed by the configured
hook shell. Its outer timeout is three seconds, the maximum accepted by Codex
0.154.0, covering PowerShell startup and the bounded helper transaction without
the former two-second failure or timeout-clamping warning. Stop success is exit
code 0 with empty stdout. Transport ACKs remain
inside the named-pipe protocol and never become provider stdout. Claude keeps
its existing command contract and is not assigned Codex response semantics.

Windows regression tests cover cancellation during pipe recreation, a connected
server that never sends ACK, and a client that deliberately delays reading ACK.
Another native test runs the generated Codex command through the default
`cmd.exe /C` path and a configured PowerShell, requires empty stdout/stderr and
verifies that the Stop event reaches its run.
They are cross-compiled and linted on macOS; execution remains part of the native
Windows CI gate.

## Terminal and process lifecycle

Windows starts with the GUI process environment and selects an explicit `SHELL`,
`pwsh.exe`, Windows PowerShell or `ComSpec`, in that order. Resolver matching for
`Path` and `PATHEXT` is case-insensitive and excludes relative PATH entries.
Native executables run directly. PowerShell, Node and command scripts use an
explicit interpreter; unsafe CMD expansion inputs are rejected with an error.

The locked Alacritty 0.26.0 source has a narrow local patch under
`vendor/alacritty_terminal`. It quotes the executable itself and accepts an
opaque Job Object handle. ConPTY passes that handle through
`PROC_THREAD_ATTRIBUTE_JOB_LIST` in the same `CreateProcessW` call, removing the
post-spawn assignment race. Every terminal job has `KILL_ON_JOB_CLOSE`; Stop and
natural-exit supervision check termination and wait until the job has no active
processes by polling `QueryInformationJobObject` with a five-second deadline.
Cleanup failure is returned through the terminal and workspace owners, blocks
removal of that worktree and keeps Quit from closing the application. The
session retains its cleanup result and Job Object after natural exit. Concurrent
Stop callers join one view operation; a later Stop retries failed verification
and clears only that pane's recorded error after confirmed success.
Cleanup operations carry the terminal generation and are discarded before a
new session starts. Removed panes remain retained with their pane and workspace
identity until cleanup succeeds, so closing UI cannot detach an unverified job
from worktree-removal guards.
Temporary-directory deletion is checked on both the initial cleanup and retry.
A Windows sharing violation retains the path and produces a cleanup error until
the handle is released and a later retry removes the directory.
Profile environment overrides replace inherited keys case-insensitively, and
the patched ConPTY encoder emits a sorted environment block without logging
values. Windows-only tests cover program paths with spaces, literal
argv and environment, CMD and PowerShell adapters, resize, final output, exit
codes, repeated Stop and the invariant that descendants are gone when session
completion is signaled.

## Data directories, credentials and private profiles

`CANOPY_DATA_DIR` is the explicit override on every platform. Without it, macOS
keeps `~/Library/Application Support/Canopy Rust`, while Windows resolves
`FOLDERID_LocalAppData` and appends `Canopy Rust`. User home is resolved
separately through `FOLDERID_Profile`; Canopy does not replace global `HOME`.
SQLite, attachment previews, task context and private agent configuration share
this data-directory decision. Windows creates those private roots with a
protected DACL for LocalSystem and the directory owner.

Agent API keys and integration tokens retain separate service namespaces. On
Windows they use generic Credential Manager entries through `CredWriteW`,
`CredReadW`, `CredDeleteW` and `CredFree`, with a 2560-byte payload limit and no
plaintext fallback. Missing-entry deletion succeeds. All calls remain blocking
operations for background executors; zero-length or malformed entries created
outside Canopy are rejected. SQLite stores opaque IDs only; failed retirement
remains in the versioned catalog and is retried on a later save or start instead
of being reported as successful cleanup. If updating cleanup state in SQLite
fails during startup, Canopy retains the previously loaded queue, displays a
warning and continues workspace initialization.

Gemini JSON overrides live in an isolated directory beneath `agent-config`.
Windows sets `USERPROFILE` only for that process and hardlinks the credential
allowlist (`oauth_creds.json`, `google_accounts.json`, `mcp-oauth-tokens.json`
and `installation_id`) so CLI refreshes reach the original profile without
Developer Mode. Other files are not copied. A cross-volume profile/data layout
fails explicitly because Windows cannot create the required hardlink.

Attachment names reject Windows device names, forbidden characters and trailing
dots or spaces while preserving a bounded UTF-8 filename and its extension.
The Win32 code is linted in the focused MSVC harness. Credential round trips,
effective DACLs, hardlink behavior and cleanup still require native Windows.

## Git hooks, signing and authentication

Commit hooks and signing programs use one bounded subprocess runner. Windows
creates each process atomically inside a `KILL_ON_JOB_CLOSE` Job Object through
`PROC_THREAD_ATTRIBUTE_JOB_LIST`; the job and inherited-handle arrays remain in
stable boxed storage until the attribute list is deleted. Only stdin, stdout and
stderr are inherited. The parent ends are private, randomly named pipes opened
with `FILE_FLAG_OVERLAPPED`. Both output streams are drained concurrently with
64 KiB retained per stream. I/O workers wait on their operation and one
persistent manual-reset stop event, so cancellation cannot fall between a stop
check and the next read/write. Cancellation, timeout, output overflow for
signers, and normal parent exit terminate descendants before bounded joins.
If the final one-second pipe drain cannot finish, the operation fails before
the stop event interrupts pending I/O; exit code zero cannot override that error.
Helper console windows are suppressed; graphical pinentry and SSH_ASKPASS may
still open their own UI.

Windows hooks may be native binaries or scripts with a bounded UTF-8 shebang.
`/bin/sh` and `/usr/bin/env sh` resolve `sh.exe` through the captured Windows
PATH, beside a discovered `git.exe`, or under `ProgramFiles\Git\bin`. A missing
interpreter fails the commit and preserves its draft. The hook receives the real
worktree, Git dir, index and author environment. `GIT_EDITOR=:` remains explicit
and has a native Git for Windows regression test. Pre-push and post-merge hooks
continue to block libgit2 network operations rather than being skipped.

OpenPGP and SSH signing resolve a native EXE/COM on Windows. Program paths and
argv are encoded independently for `CreateProcessW`; cwd and Git paths retain
their OS-string representation. The 120-second deadline, no unsigned fallback,
post-sign HEAD/index validation and post-commit warning contract are unchanged.

Windows HTTPS lookup reads one exact Generic Credential target:
`git:https://host[:port]`, or `git:https://host[:port]/path` when
`credential.useHttpPath=true`. It accepts UTF-8 and GCM-style UTF-16 credential
blobs, uses the configured/URL/stored username, and never consults tracker-token
namespaces or executes a credential helper. An explicit username must exactly
match the username stored with the selected credential. The error names the
expected target so it can be created through Git Credential Manager or Windows
Credential Manager. URL-scoped `credential "https://…"` variants are not yet
resolved; the current path policy reads the repository's general
`credential.useHttpPath`. The resolved Windows feature graph enables
`git2`/libgit2 SSH through `libssh2-sys 0.3.1`. The default WinCNG branch compiled
RSA private-key loading without `HAVE_LIBCRYPT32`, so file authentication ended
as `Method unsupported in Windows CNG backend` while Git for Windows/OpenSSH
could use the same key. Canopy feature-unifies that locked dependency with
`openssl-on-win32` and `vendored-openssl`. The resulting build retains Pageant
and Windows OpenSSH named-pipe backends and can load RSA/OpenSSH key files; a
native handshake is still required to prove which backend is active in a
particular packaged EXE. Canopy retains the bounded agent then default-key order.
For SSH, an explicit remote username wins and the fallback is `git`; the general
HTTPS `credential.username` is never reused as an SSH login. Authentication
diagnostics survive the final libgit2 error and distinguish missing agent/keys,
encrypted default keys and server rejection. The message includes the git2
code/class and cannot end with an empty colon. When `~/.ssh/config` exists the
error explicitly states that Host, User and IdentityFile are not interpreted.
Certificate and host-key verification are not bypassed.

Native user verification after enabling the OpenSSL backend confirmed Pull from
the same SSH remote that Git for Windows could access. This qualifies the default
RSA file path for that repository; Pageant, agent-only identities, encrypted
keys, custom IdentityFile and other servers remain separate acceptance cases.

Focused MSVC Clippy covers the Win32 runner, Job Object and Credential Manager
code plus native-only regression definitions. Execution with Git for Windows,
real GPG/pinentry, OpenSSH/Pageant and GCM remains a native acceptance gate.

## Windows notch

The session projection, filters, notice timing, fixed 480-unit canvas and motion
remain shared with macOS. `InputRegion` is pure geometry in
`notch_geometry.rs`; the controller owns the only session list and updates the
native region from the current animation values. An empty session list installs
an empty region immediately.

The Win32 adapter obtains only the notch HWND from `HasWindowHandle`. It adds
`WS_EX_NOACTIVATE`, keeps TOOLWINDOW/TOPMOST, uses `SWP_NOACTIVATE`, and installs
a scoped subclass returning `MA_NOACTIVATE` for `WM_MOUSEACTIVATE`. Other
messages go through `DefSubclassProc`; `WM_NCDESTROY` removes the subclass.
Clicking a session remains the only path that activates and restores the main
window through GPUI.

Click-through uses `SetWindowRgn`, not `HTTRANSPARENT` or window alpha. The
region has square top corners and the current animated bottom radius. It is
recomputed in physical pixels for the window DPI without resizing the fixed
HWND. A UI-thread `WH_MOUSE_LL` hook observes pointer movement even outside the
window region, reads no keyboard input and stores no pointer history. Region
updates re-evaluate a stationary cursor. Installation failure closes the overlay,
reactivates the main window and shows an in-app message; a later region failure
hides the HWND rather than leaving a stale blocking region. Runtime failure is
also stored in `NotchStatus` and remains visible in the main status bar. The
controller performs one delayed retry, while later display/foreground events
provide event-driven retry; successful recovery clears the persistent error.
Render-driven `set_region` only stores the newest geometry while unavailable;
it never retries native calls per frame. The same persistent error is shown in
the main empty state when no project is selected.

The primary-monitor anchor uses the current work area when a taskbar occupies
the top edge. `WM_DPICHANGED`, `WM_DISPLAYCHANGE` and `WM_SETTINGCHANGE`
re-anchor and reapply the region. Pure tests cover DPI scaling, negative monitor
origins, a top taskbar and narrow-work-area clamping. A scoped foreground event
hook plus filtered `EVENT_OBJECT_STATECHANGE`/`LOCATIONCHANGE` observations
handle F11 enter/exit without an HWND switch. Child-object events are ignored.
Repeated events coalesce into one private HWND message per message-loop turn;
the WinEvent callback does not perform geometry work itself.
The notch hides only when the foreign borderless fullscreen window covers the
same monitor as the notch; fullscreen on a secondary monitor, maximized windows
and Canopy-owned windows remain visible. Default Windows
virtual-desktop ownership is retained; secure desktop/UAC is outside scope.

`CANOPY_NOTCH_PREVIEW` is enabled on Windows. The focused MSVC harness imports
the actual geometry and adapter modules and passes Clippy. Native acceptance
still requires a real pointer over another process, browser-tab click-through,
scroll/filter behavior, no foreground change on hover, minimized-main restore,
100/125/150/200% DPI, monitor reconnect, fullscreen and hook/HWND cleanup.

## Windows chrome, input and accessibility

The shared 40-unit titlebar now supplies native Windows hit-test areas for
Minimize, Maximize/Restore and Close. `WindowControlArea::Max` maps through GPUI
to `HTMAXBUTTON`, preserving the system Snap Layout affordance. Main-window
Close and Alt+F4 continue through the existing dirty-buffer, final-save and
process-cleanup guard. Preferences and attachment preview use the same caption
component. Windows omits the macOS traffic-light inset; the product sidebar
controls reserve the caption-button lane instead.

GPUI 0.3.4 evaluates window-control hitboxes in insertion order. A `Drag` area
on the titlebar parent therefore won over every descendant and turned both
application buttons and Min/Max/Close into `HTCAPTION`. The titlebar now uses a
separate central Drag rectangle whose bounds do not overlap either workspace
toggle or the three caption controls. Caption clicks remain native
`WM_NCLBUTTONDOWN/UP` actions, so each action executes once, Max keeps Snap
Layouts, and Close still reaches the existing window-close callback. Secondary
Preferences and attachment windows use the same disjoint caption/drag layout.
Pure geometry tests cover all five button centers and the remaining draggable
center; native clicks, drag and double-click still require Windows GUI testing.

macOS keeps its existing Cmd bindings. Windows workspace actions use explicit
Ctrl+Shift combinations, including Ctrl+Shift+T/W/P/S and Ctrl+Shift+A for the
Session inspector. Terminal copy/paste use Ctrl+Shift+C/V. Unshifted Ctrl+C,
Ctrl+D, Ctrl+P, Ctrl+S, Ctrl+W and Ctrl+Z remain available to the PTY. A
Ctrl+Alt/AltGr event follows the native text/IME path only when the Windows
backend sets `prefer_character_input` and reports nonempty `key_char`. Ctrl+Alt
without text and ordinary Alt+B/Alt+F remain available to the PTY. Preferences
Search uses Ctrl+K and visible shortcut copy is platform-specific.
File Editor adds a narrower `FileEditor` Ctrl+S binding which calls the existing
atomic save path for the focused pane. It does not restore a workspace-level
unshifted binding, so terminal Ctrl+S still reaches the PTY.

The interface font is Segoe UI on Windows; the embedded JetBrains Mono terminal
font and fixed glyph grid are unchanged. The main window owns a scoped
`WM_SETTINGCHANGE` subclass. `SPI_GETCLIENTAREAANIMATION` is read at startup and
on that event, cached outside render, and used by `motion::policy`. Switching to
system Reduce Motion makes an in-flight Transition/Presence return its target
and stop requesting frames immediately. Re-enabling animations does not resume
that old transition; a new target is required.

The Session inspector is the keyboard alternative to the nonactivating notch.
Its shortcut moves focus from the terminal to the active-or-unseen session list.
Arrow keys select an entry, Enter calls the same `AgentsState::focus` path as
the notch and Escape restores the prior terminal focus. The focused empty state
also accepts Escape. Entries keep stable PaneId-derived identities and explicit
accessibility names.

Unit tests cover AltGr/dead-key/Alt navigation routing and render the real
Workspace, SessionInspector and TerminalViews through AgentsState and Terminals.
They also cover empty-list focus return and in-flight Reduced Motion. The
focused MSVC harness covers `SPI_GETCLIENTAREAANIMATION`
and the scoped settings subclass. A full titlebar cross-check cannot pass
`aws-lc-sys`/`ring` on the
macOS host without Windows SDK headers. Native acceptance still requires Polish
input and IME, caption hover/click, Snap, drag/double-click, maximize bounds,
Alt+F4 guards, screen readers and live system animation changes.

## Windows filesystem and worktree cleanup

File access canonicalizes the selected root and every existing path component.
On Windows it rejects every `FILE_ATTRIBUTE_REPARSE_POINT`, including junctions,
before reading, creating or replacing a file. The comparison remains component
and case exact after canonicalization; Canopy does not lowercase drive, UNC or
case-sensitive-directory paths. Missing worktree identity canonicalizes the
deepest existing parent. Only `NotFound` qualifies as missing; access and sharing
errors keep the registration protected.

Canonical Win32 paths keep their `\\?\` prefix in state, persistence, Git,
comparisons and I/O. Presentation-only labels and tooltips convert documented
`\\?\C:\...` paths to `C:\...` and `\\?\UNC\server\share` to
`\\server\share`. Device, volume and GLOBALROOT namespaces stay unchanged.
Copy path in the worktree dialog deliberately retains the exact system path so
long-path operations remain unambiguous.

Editor save retains UTF-8 BOM, CRLF and permissions, checks the loaded bytes both
before writing and immediately before publication, then uses same-directory
`MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`. A read-only target, antivirus or
handle without delete sharing leaves the original and dirty buffer intact for
retry. The final compare/replace interval is still an optimistic concurrency
boundary rather than a filesystem lock against unrelated writers.

Notify overflow/rescan and runtime errors request one coalesced full refresh and
leave a visible warning. There is no idle polling. Immediately before worktree
mutation, Canopy pauses the active Files watcher, editor/preview watchers and the
selected Changes watcher. Failed analysis, sharing errors and confirmation steps
restore them; successful removal closes the workspace without reopening handles.

Preferences stores argv as an array and now uses reversible Windows command-line
quoting only for editing that array. Backslashes are not POSIX escapes. Terminal
drop formats paths for the active POSIX, PowerShell or CMD dialect, preserves one
bracketed-paste frame and sends no Enter. Every CMD path is double-quoted so
`&|<>^()` remain literal; paths containing `%` or `!` are rejected rather than
expanded. Task attachment references use explicit
Claude/Codex `@` grammar and quote Windows paths without passing them through a
shell.

Pausing a clean editor for worktree cleanup retains its `EditorChanged`
subscription and in-memory editor state while releasing only filesystem tasks.
After a failed cleanup, the watcher resumes and later dirty/saving transitions
still update the owning tab and close guards.

Detailed work packages, dependencies and final manual matrix remain in
[`windows-implementation-plan.md`](windows-implementation-plan.md).
