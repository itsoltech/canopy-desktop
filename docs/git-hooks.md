# Commit hooks

Canopy runs executable `pre-commit`, `prepare-commit-msg`, `commit-msg` and
`post-commit` hooks on the existing Git worker. It honors absolute or worktree-root
relative `core.hooksPath`; otherwise it uses the repository's shared hooks folder.
Missing hooks are ignored. Unix also requires the executable bit; Windows
accepts a regular hook file because NTFS has no equivalent mode bit. Inspection
and execution errors are reported rather than silently bypassed. There is no
no-verify UI switch.

The order is pre-commit, prepare-commit-msg, commit-msg, signing/publication,
post-commit. Prepare receives the message file path and `message` source, commit-msg
receives that path, and pre/post receive no arguments. A private temporary file in
the worktree Git directory carries the message, avoiding overwriting another
process's COMMIT_EDITMSG. Hooks must use the message path passed in argv.

Hook cwd is the working-tree root. The captured shell environment supplies PATH
and tool configuration; repository-local Git environment is replaced with the
correct GIT_DIR, GIT_WORK_TREE and GIT_INDEX_FILE. GIT_EDITOR=: and the commit author
are passed explicitly. Hook subprocesses may invoke git/npm/etc. themselves; all
Git operations implemented by Canopy still use libgit2. On Unix, executable
scripts without a shebang use `/bin/sh` only after ENOEXEC. Windows scripts need
a UTF-8 shebang no longer than 4096 bytes; `/bin/sh` and `/usr/bin/env sh`
resolve `sh.exe` through PATH, next to a discovered `git.exe`, or from the
standard `ProgramFiles\Git\bin` location. A missing interpreter is a blocking
error that preserves the draft. Native extensionless executables run directly.

Pre-publication failures/cancellation stop Canopy's commit and keep the user's
message draft. Hook filesystem/index edits remain visible and are never reverted
implicitly. After hooks, Canopy reloads the index and message; signing covers this
final content. HEAD/repository state is checked after hooks, and HEAD/index are
checked again with publication locks after signing. No index/ref locks are held
while user hooks or signing prompts run.

Post-commit runs after publication and release of locks. Its failure returns a
successful commit ID plus a warning, not a failed/rolled-back commit. The UI clears
the completed draft, shows the warning and offers the captured hook output.
During the operation the commit button shows the running hook/signing stage;
Cancel commit and application quit use the existing cancellation token.

Limits: each hook has a 5-minute deadline, null stdin (no terminal prompting),
64 KiB retained per output stream and a truncation marker. Output is drained
without blocking the worker on full pipes. Unix uses a dedicated process group;
Windows assigns the process atomically to a Job Object and inherits only three
handles backed by private overlapped named pipes. A persistent stop event is
observed together with each read/write operation; descendants are stopped before
bounded reader/writer joins on normal exit, cancellation and timeout. Persistent
background services launched by hooks are not a supported use case. A final
drain exceeding one second is an operation error even when the hook returned
exit code zero. Scripts can have arbitrary side
effects; stopping a hook cannot undo work it already performed. General reference
transaction hooks, amend/post-rewrite and merge/rebase hooks are outside this
normal staged-commit path.

Verification uses disposable repositories and hooks: order/argv/environment,
relative/absolute hooks paths, edited index/message, no unintended staging,
rejection, cancellation, shell fallback, ignored non-executable hooks,
post-commit warning, changed HEAD and linked-worktree isolation. Existing signing
regressions also passed. Windows-native definitions additionally cover Git for
Windows shebangs, `GIT_EDITOR=:`, a missing interpreter and cancellation with a
descendant. They remain unexecuted until the native runner is available. Agent
resume checks were not repeated.

Reference: https://git-scm.com/docs/githooks

Release GUI checks in an isolated repository confirmed the current phase label,
commit-msg rejection with unchanged draft, post-commit failure with a successful
commit and cleared draft, yellow warning, and expandable hook logs. Inspection of
the resulting commit confirmed the message edit performed by commit-msg. The test
commit was 57bbd60 in the disposable repository, not the Canopy source checkout.
