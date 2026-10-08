# Contributing

Desktop development happens on `rust-rewrite` using Rust + GPUI Kit. Preserve the
pinned toolchain, `Cargo.lock`, product contracts in [AGENTS.md](AGENTS.md), and
unrelated local changes.

## Who can contribute what

Canopy is source-available software. Per the [license](LICENSE.md), code
contributions (pull requests, patches) are accepted only from employees or
authorized contractors of IT SOL Sp. z o.o.

Everyone can report bugs, suggest features using the repository's issue
templates, and join discussions.

## Reporting bugs and suggesting features

Bug reports should include minimal numbered reproduction steps, platform,
app version, and screenshots or recordings when applicable. The existing bug
assignment workflow uses Git history for triage.

Feature requests must explain the workflow problem, who benefits, and whether
the feature should be opt-in or enabled by default. Features that add UI
complexity without solving a concrete workflow problem, affect the default
experience for a niche audience, or duplicate existing functionality are not
accepted. Non-core features stay behind a feature flag, off by default; users
opt in. Core security fixes, critical UX and essential workflows may be enabled
by default.

## Desktop checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --lib
```

For a local macOS release bundle, run `./scripts/build-macos.sh release`.
`./scripts/run-macos.sh release` also opens it. GUI/E2E verification is separate
from unit tests and requires prior authorization under the workspace rules.

## Mobile

The independent app keeps its own dependencies, lockfile and EAS configuration.
Run npm commands inside `mobile/`; see [mobile/README.md](mobile/README.md).
Its Remote Control backend still requires the Electron implementation on `next`.

## Commits and reviews

Use conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`,
`build:`), with a title under 72 characters and lowercase after the prefix.
Branch from `rust-rewrite` for desktop work. Keep PRs focused and fill in What,
Why and How to test; include screenshots or recordings for UI changes.
State what was verified and which platform/runtime checks remain open.
The existing PR labels and auto-merge gate remain in place. Electron-specific
automated code review and auto-fix workflows were removed; approval labels must
be applied after review.

## Privacy and security

- Never log secrets, passwords, tokens or API keys in logs or diagnostics.
- Store credentials in the OS keychain, rather than plaintext.
- Encrypt sensitive user data before persisting it.
- Telemetry and analytics require explicit user consent.
- Sanitize user data in error reports and diagnostics.

## Documentation

Code and docs ship together. A PR changing behavior, configuration, errors or
security properties must update the relevant document in `docs/` in the same
PR. New feature domains need a document covering behavior, configuration,
errors, security/privacy and source files. Historical verification is separate
from current test results.

## AI policy

AI tools are allowed and held to the same review standards as hand-written work.

- Understand every line you submit and be able to explain your changes.
- Disclose AI tools used in the PR description.
- Take responsibility for correctness, security, privacy and licensing.
- AI-generated media (images, icons, assets) requires prior approval.
- Poor-quality contributions may be rejected and repeat offenses may restrict access.

## Responsibility

You own your changes and fix regressions. Security and privacy violations are
not accepted. Review the PR checklist, ensure CI passes and address review
findings before requesting approval.
