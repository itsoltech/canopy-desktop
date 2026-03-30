## [0.7.0-next.7](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.6...v0.7.0-next.7) (2026-03-30)

### Bug Fixes

* **updater:** bypass unreliable quitAndInstall on macOS ([#27](https://github.com/itsoltech/canopy-desktop/issues/27)) ([ff2ce6f](https://github.com/itsoltech/canopy-desktop/commit/ff2ce6f0cf12eef6ac2c48617ad2ac55343d7ab6))

## [0.7.0-next.6](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.5...v0.7.0-next.6) (2026-03-30)

### Bug Fixes

* **terminal:** preserve scroll position when new content arrives ([#26](https://github.com/itsoltech/canopy-desktop/issues/26)) ([7507315](https://github.com/itsoltech/canopy-desktop/commit/75073154541f69fbea0284497add5c72a273a8ca))

## [0.7.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.4...v0.7.0-next.5) (2026-03-30)

### Features

* **sidebar:** agent status indicators on worktree items ([#25](https://github.com/itsoltech/canopy-desktop/issues/25)) ([52f8238](https://github.com/itsoltech/canopy-desktop/commit/52f82386c86e496c9de1e6284fb9ee0758d59216))

### Bug Fixes

* **notch:** remove 5-item cap and fix height calculation in agent list ([#24](https://github.com/itsoltech/canopy-desktop/issues/24)) ([224595f](https://github.com/itsoltech/canopy-desktop/commit/224595f48454ae856b081241fe3d4b25289040e7))

## [0.7.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.3...v0.7.0-next.4) (2026-03-30)

### Bug Fixes

* **ci:** add --auto flag to auto-merge gh pr merge command ([2028541](https://github.com/itsoltech/canopy-desktop/commit/20285418dd83c411141553bbe179fea8420364d9))

## [0.7.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.2...v0.7.0-next.3) (2026-03-30)

### Features

* post-update changelog modal with release notes ([#22](https://github.com/itsoltech/canopy-desktop/issues/22)) ([2575ef6](https://github.com/itsoltech/canopy-desktop/commit/2575ef69d2cc6e21d165163efb94529426eb5e11))

## [0.7.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.1...v0.7.0-next.2) (2026-03-29)

### Refactoring

* **sidebar:** remove file tree section from sidebar ([c24b1da](https://github.com/itsoltech/canopy-desktop/commit/c24b1da031c4b877893b88d3d62ccfd581fa800f))

## [0.7.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.6.2...v0.7.0-next.1) (2026-03-29)

### Features

* add file browser with read-only editor pane ([#18](https://github.com/itsoltech/canopy-desktop/issues/18)) ([9b5fb27](https://github.com/itsoltech/canopy-desktop/commit/9b5fb275b42c74f120bf8bb8f021676cb40a1440))
* next branch pre-release pipeline and update channel settings ([#19](https://github.com/itsoltech/canopy-desktop/issues/19)) ([528b0c5](https://github.com/itsoltech/canopy-desktop/commit/528b0c599cb0354a43500fbf377ed053e224f877))

### Bug Fixes

* **ci:** correct required label name to claude:review:approved ([c5f1758](https://github.com/itsoltech/canopy-desktop/commit/c5f1758b1d44e954c6c1db6847ee013b1f5cc5ac))
* **ci:** use PAT for semantic-release to bypass branch ruleset ([4c367e2](https://github.com/itsoltech/canopy-desktop/commit/4c367e2bca7a58ec4b11afb59833f5c36c1095fc))

## [0.6.2](https://github.com/itsoltech/canopy-desktop/compare/v0.6.1...v0.6.2) (2026-03-29)

### Bug Fixes

* **notch:** eliminate first-hover FPS drop ([#16](https://github.com/itsoltech/canopy-desktop/issues/16)) ([84d55a0](https://github.com/itsoltech/canopy-desktop/commit/84d55a06ff2ed873e12be22436810e0d7acd7526))
* **worktree:** dispose PTY sessions before removing worktree ([#17](https://github.com/itsoltech/canopy-desktop/issues/17)) ([ed2bb02](https://github.com/itsoltech/canopy-desktop/commit/ed2bb02ead0d6dbbdee6860cbe4d799cd45b13df))

## [0.6.1](https://github.com/itsoltech/canopy-desktop/compare/v0.6.0...v0.6.1) (2026-03-29)

### Bug Fixes

* **claude:** restore command hooks, keep only agent hooks removed ([6203b7f](https://github.com/itsoltech/canopy-desktop/commit/6203b7f6131121ea55ed59672675dfc1cb5acd71))
* **deps:** patch brace-expansion and picomatch vulnerabilities ([#15](https://github.com/itsoltech/canopy-desktop/issues/15)) ([ab65db2](https://github.com/itsoltech/canopy-desktop/commit/ab65db237c0db4a1b072a0e513e185a64ce8f122))
* **notch:** check active Claude tab before suppressing peek ([8ad0ec9](https://github.com/itsoltech/canopy-desktop/commit/8ad0ec9ed1b5cfad62d52eb8e717b2e5d394a081))
* **notch:** correct status mismatch and suppress peek for focused window ([5541b53](https://github.com/itsoltech/canopy-desktop/commit/5541b539fbd2764f5f53614de2c51d35019b81e6))

### Refactoring

* **notch:** remove session count from collapsed header ([fde97eb](https://github.com/itsoltech/canopy-desktop/commit/fde97ebf1e030254da5cf4e73464153c54391b26))

## [0.6.0](https://github.com/itsoltech/canopy-desktop/compare/v0.5.2...v0.6.0) (2026-03-29)

### Features

* **notch:** macOS notch overlay for Claude session status ([#13](https://github.com/itsoltech/canopy-desktop/issues/13)) ([d96e5e6](https://github.com/itsoltech/canopy-desktop/commit/d96e5e6b5fcd97c8721a0425a04f43ee7ceb0149))

### Bug Fixes

* **e2e:** use specific selector for reopen workspace checkbox ([4bf41a9](https://github.com/itsoltech/canopy-desktop/commit/4bf41a9934a5749872c0450fa891b938f605d1b9))
* **workspace:** restore all projects and active worktree on update restart ([#12](https://github.com/itsoltech/canopy-desktop/issues/12)) ([c62cde1](https://github.com/itsoltech/canopy-desktop/commit/c62cde1f924f984e9f526ea81875b33672801c65))

### Styling

* **css:** add tabular-nums for stable digit widths ([ac1cf6e](https://github.com/itsoltech/canopy-desktop/commit/ac1cf6e3f9c01122339c86053ff303236016b171))

## [0.5.2](https://github.com/itsoltech/canopy-desktop/compare/v0.5.1...v0.5.2) (2026-03-28)

### Bug Fixes

* **ci:** allow claude and dependabot actors in codebase audit ([6e7012d](https://github.com/itsoltech/canopy-desktop/commit/6e7012d0111f0afd0f60449a70cff6e7281fad03))
* **ci:** use valid prompt input for claude-code-action ([0df2982](https://github.com/itsoltech/canopy-desktop/commit/0df298239efb50f0f18dab3589955f96919e7a2e))
* **security:** harden Electron IPC, shell execution, and credential storage ([#4](https://github.com/itsoltech/canopy-desktop/issues/4)) ([f430c88](https://github.com/itsoltech/canopy-desktop/commit/f430c88c475226f2d54f634426b153673aeb3918))

## [0.5.1](https://github.com/itsoltech/canopy-desktop/compare/v0.5.0...v0.5.1) (2026-03-28)

### Bug Fixes

* **ui:** move [@const](https://github.com/const) declarations to valid [#each](https://github.com/itsoltech/canopy-desktop/issues/each) block scope ([de6b8e2](https://github.com/itsoltech/canopy-desktop/commit/de6b8e20cb370cc933b1a4f43107529fcaa9dbb3))

## [0.5.0](https://github.com/itsoltech/canopy-desktop/compare/v0.4.0...v0.5.0) (2026-03-28)

### Features

* **ui:** universal panel split with drag & drop ([#9](https://github.com/itsoltech/canopy-desktop/issues/9)) ([76a1729](https://github.com/itsoltech/canopy-desktop/commit/76a1729838269ebba89c56d260c64f75e6bdab16))

### Bug Fixes

* **layout:** persist layout when Claude session ID changes mid-session ([91aa2a5](https://github.com/itsoltech/canopy-desktop/commit/91aa2a5958374eea89aa5bd376b3005967cbe522))
* **release:** checkout github.sha instead of empty ref for dry-run ([626e329](https://github.com/itsoltech/canopy-desktop/commit/626e3292587abf05a428458a577139628b37e7d1))
* **release:** use triggering commit ref for dry-run builds ([81653b1](https://github.com/itsoltech/canopy-desktop/commit/81653b185d7b4fdcbff43945f0b81c6865f34c71))
* **terminal:** show reconnection status as tab badge instead of writing to terminal ([919cde4](https://github.com/itsoltech/canopy-desktop/commit/919cde49f75a1bc45428eecbba8693f65f9b7861))
