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
