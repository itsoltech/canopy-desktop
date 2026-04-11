## What

<!-- Short description of the change. -->

## Why

<!-- What problem does this solve? Link related issues with #123. -->

## How to test

<!-- Steps for the reviewer to verify this change. -->

1.

## Screenshots / recordings

<!-- Optional. Attach for UI changes. Delete this section if not applicable. -->

## Checklist

- [ ] No secrets, tokens, or credentials logged or stored in plaintext
- [ ] Non-core feature is behind a feature flag (off by default)
- [ ] Cross-platform: no hardcoded OS-specific labels, paths, or shell commands
- [ ] Keyboard accessible (all interactive elements reachable via keyboard)
- [ ] IPC follows `feature:action` naming and uses `invoke`/`handle`
- [ ] Renderer code does not import Node.js modules directly
- [ ] Behavior changes reflected in `docs/` (or N/A)
