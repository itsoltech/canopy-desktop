## [0.11.0-next.8](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.7...v0.11.0-next.8) (2026-04-13)

### Bug Fixes

* **git:** coalesce GitWatcher refreshes to fix shutdown crash ([#147](https://github.com/itsoltech/canopy-desktop/issues/147)) ([#148](https://github.com/itsoltech/canopy-desktop/issues/148)) ([05b4467](https://github.com/itsoltech/canopy-desktop/commit/05b4467de482bea4d75325ce46386075da1e77b5))

## [0.11.0-next.7](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.6...v0.11.0-next.7) (2026-04-13)

### Features

* **agents:** named profiles per AI agent for multi-provider ([#146](https://github.com/itsoltech/canopy-desktop/issues/146)) ([c28e40b](https://github.com/itsoltech/canopy-desktop/commit/c28e40b12f3c4d96da7198b4972b94e77753aff5))

### Bug Fixes

* **remote:** grant pty.write/agent.sendInput once per session ([#145](https://github.com/itsoltech/canopy-desktop/issues/145)) ([f11b868](https://github.com/itsoltech/canopy-desktop/commit/f11b868212dd6b4b19821b59e5f5192b0e1b323c)), closes [#136](https://github.com/itsoltech/canopy-desktop/issues/136)

## [0.11.0-next.6](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.5...v0.11.0-next.6) (2026-04-10)

### Bug Fixes

* **ci:** queue concurrent code review runs instead of cancelling ([c43c40c](https://github.com/itsoltech/canopy-desktop/commit/c43c40c0faaf694fbebd899a8a6682d4be97b0e3))
* **terminal:** debounce resize to prevent flickering ([#142](https://github.com/itsoltech/canopy-desktop/issues/142)) ([3bef79e](https://github.com/itsoltech/canopy-desktop/commit/3bef79eca725ea72cf9748af9834a3fc2cd37438))

## [0.11.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.4...v0.11.0-next.5) (2026-04-10)

### Features

* **agents:** add Codex agent integration ([#138](https://github.com/itsoltech/canopy-desktop/issues/138)) ([b4c2ada](https://github.com/itsoltech/canopy-desktop/commit/b4c2adad9d9cb82bc488dfd9b6a72feeacaeb2aa))
* **agents:** add OpenCode integration via plugin bridge ([#139](https://github.com/itsoltech/canopy-desktop/issues/139)) ([28ac4d4](https://github.com/itsoltech/canopy-desktop/commit/28ac4d4ac6b4a4642e4e3f6d4239b5f97a96647e))
* crash detection with GitHub issue reporting ([#140](https://github.com/itsoltech/canopy-desktop/issues/140)) ([54639dd](https://github.com/itsoltech/canopy-desktop/commit/54639dd22de6bf9f94bfeb7e8b68d97c00e63fe8))
* **ui:** add pane drag-and-drop for reorder, move, and detach ([#137](https://github.com/itsoltech/canopy-desktop/issues/137)) ([518e47d](https://github.com/itsoltech/canopy-desktop/commit/518e47df97ea32742bb911c3000d2398c20ef31b))

### Bug Fixes

* **ci:** add id-token write permission to claude-code-compat workflow ([3f76be8](https://github.com/itsoltech/canopy-desktop/commit/3f76be8430d9cdf2bbfe29bc12e46ca7ab169a8f))
* **ci:** hard reset before updating claude-code version tracker ([3b1b5d9](https://github.com/itsoltech/canopy-desktop/commit/3b1b5d91497cd152724a15c958409ec4c7aef92a))
* **ci:** rebase compat branch after version tracker bump ([2f12e7b](https://github.com/itsoltech/canopy-desktop/commit/2f12e7b4eddb2f1607812d7bcc963a36aadb421a))
* **telemetry:** deduplicate daily ping and disable in dev mode ([#141](https://github.com/itsoltech/canopy-desktop/issues/141)) ([70b094f](https://github.com/itsoltech/canopy-desktop/commit/70b094f03ffb303c36c37711a324b507c1f2c11b))

## [0.11.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.3...v0.11.0-next.4) (2026-04-09)

### Bug Fixes

* **ui:** preference hints, PR creation, sidebar and runtime fixes ([#132](https://github.com/itsoltech/canopy-desktop/issues/132)) ([87a94eb](https://github.com/itsoltech/canopy-desktop/commit/87a94eb5ee0e79eebd28136b4abfae77594f31e6))

## [0.11.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.2...v0.11.0-next.3) (2026-04-09)

### Features

* add toggleable cpu/ram hud in status bar ([#131](https://github.com/itsoltech/canopy-desktop/issues/131)) ([b2c18a9](https://github.com/itsoltech/canopy-desktop/commit/b2c18a960405074acd80290b75c0aacc8dead52f))

### Bug Fixes

* **workspace:** clean up stale project rows on startup restore ([#130](https://github.com/itsoltech/canopy-desktop/issues/130)) ([38fd658](https://github.com/itsoltech/canopy-desktop/commit/38fd658e6ab84e059963084fcb0b5d3cd88a692f)), closes [#128](https://github.com/itsoltech/canopy-desktop/issues/128)

## [0.11.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.11.0-next.1...v0.11.0-next.2) (2026-04-09)

### Features

* **remote:** webrtc remote control via qr pairing (beta) ([#129](https://github.com/itsoltech/canopy-desktop/issues/129)) ([e6848f0](https://github.com/itsoltech/canopy-desktop/commit/e6848f06a5c7b4b2255bdfa0b55fc14039e5c7ec))

## [0.11.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0...v0.11.0-next.1) (2026-04-09)

### Features

* add run configurations system with .canopy/run.toml ([#126](https://github.com/itsoltech/canopy-desktop/issues/126)) ([865c4e4](https://github.com/itsoltech/canopy-desktop/commit/865c4e467644833199adf0fc1df7ca4c6a682642))

## [0.10.0](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0...v0.10.0) (2026-04-09)

### Features

* add diff review panel with tabbed right sidebar ([#96](https://github.com/itsoltech/canopy-desktop/issues/96)) ([8945a44](https://github.com/itsoltech/canopy-desktop/commit/8945a447b7b0daa179a3c7e9361b3123d4743abe))
* add keystroke visualizer overlay with WPM tracking ([#59](https://github.com/itsoltech/canopy-desktop/issues/59)) ([5bd0c5d](https://github.com/itsoltech/canopy-desktop/commit/5bd0c5d25738f5da1d452585d686d02af6ec5316))
* add performance testing infrastructure ([#92](https://github.com/itsoltech/canopy-desktop/issues/92)) ([4df7adc](https://github.com/itsoltech/canopy-desktop/commit/4df7adcdb9a357e18261b8f09f3a48857699ffd2))
* **ci:** auto-track claude code releases for compatibility ([#121](https://github.com/itsoltech/canopy-desktop/issues/121)) ([338d5a2](https://github.com/itsoltech/canopy-desktop/commit/338d5a209eb513d91fc2b279d75542b4acfd7c33))
* **github:** add GitHub integration with GraphQL API ([#97](https://github.com/itsoltech/canopy-desktop/issues/97)) ([36626f8](https://github.com/itsoltech/canopy-desktop/commit/36626f8164d6365600bba077b4b76215c1b8a173))
* **sidebar:** dynamic file tree via native filesystem watcher ([#114](https://github.com/itsoltech/canopy-desktop/issues/114)) ([4a48626](https://github.com/itsoltech/canopy-desktop/commit/4a4862616c30844552fb03fd9227e573c031d120))
* **tasks:** repo-level config, keychain credentials, PR workflow ([#94](https://github.com/itsoltech/canopy-desktop/issues/94)) ([14c513f](https://github.com/itsoltech/canopy-desktop/commit/14c513f662325c22c32aad84a3239c9f158631ed))
* **tasks:** send task context to agent sessions ([#90](https://github.com/itsoltech/canopy-desktop/issues/90)) ([7f05068](https://github.com/itsoltech/canopy-desktop/commit/7f05068316538421d649a6821bde47bb71041f98))
* **taskTracker:** restore dual-layer config and multi-tracker support ([#118](https://github.com/itsoltech/canopy-desktop/issues/118)) ([30a28ff](https://github.com/itsoltech/canopy-desktop/commit/30a28ff2964ebf09ad7f3c8805cf5052e3c4e03f)), closes [#94](https://github.com/itsoltech/canopy-desktop/issues/94) [#94](https://github.com/itsoltech/canopy-desktop/issues/94)
* **telemetry:** add daily usage ping via Umami with opt-out in settings ([612cefd](https://github.com/itsoltech/canopy-desktop/commit/612cefd0844ad5c399099090a899352c7a5ee152))
* **updater:** configurable auto-update check frequency ([#122](https://github.com/itsoltech/canopy-desktop/issues/122)) ([62aced6](https://github.com/itsoltech/canopy-desktop/commit/62aced67207eb362d49c3d748640588297290973))
* **window:** include active tab name in window title ([#116](https://github.com/itsoltech/canopy-desktop/issues/116)) ([5f29b71](https://github.com/itsoltech/canopy-desktop/commit/5f29b71c4af9b02ad2082c3a4ca2936eaf44c6f3))
* **window:** restore window position, size, and state across restarts ([#111](https://github.com/itsoltech/canopy-desktop/issues/111)) ([1cac80e](https://github.com/itsoltech/canopy-desktop/commit/1cac80e5288d09927bacd061be34012d48f7a535))
* **worktree:** create worktree from existing branch ([#125](https://github.com/itsoltech/canopy-desktop/issues/125)) ([1b933dd](https://github.com/itsoltech/canopy-desktop/commit/1b933dd67eb4b5918923eae10ae39bdd508af261))

### Bug Fixes

* **diff:** prevent layout jump when clicking refresh button ([#107](https://github.com/itsoltech/canopy-desktop/issues/107)) ([e9a7ed0](https://github.com/itsoltech/canopy-desktop/commit/e9a7ed075ce1fee067c3eb53dad30e14cc696ab6))
* **diff:** replace emoji icons, fix tab flash, comment form polish ([#105](https://github.com/itsoltech/canopy-desktop/issues/105)) ([211e165](https://github.com/itsoltech/canopy-desktop/commit/211e16525037e651d5bb67ce5ee8ebdc5eaa6096))
* **diff:** sticky header, untracked files, scroll and layout issues ([#113](https://github.com/itsoltech/canopy-desktop/issues/113)) ([a8d3314](https://github.com/itsoltech/canopy-desktop/commit/a8d33140abdfaa0ccc9cd316d0cdcba7d1007f75))
* **git,ui:** resilient git detection and full-width preference selects ([#117](https://github.com/itsoltech/canopy-desktop/issues/117)) ([49d0d7c](https://github.com/itsoltech/canopy-desktop/commit/49d0d7c030c3b406cdb4b6c84d1dcc28cd31ff06))
* **git:** refresh worktree list on branch checkout ([#109](https://github.com/itsoltech/canopy-desktop/issues/109)) ([a9604af](https://github.com/itsoltech/canopy-desktop/commit/a9604af636ca8aff61c9b82261db9f64cd51fd63)), closes [#108](https://github.com/itsoltech/canopy-desktop/issues/108)
* **git:** sanitize branch and worktree names to strip unsafe characters ([#110](https://github.com/itsoltech/canopy-desktop/issues/110)) ([c3822f4](https://github.com/itsoltech/canopy-desktop/commit/c3822f42ca8e64ebb228c8be336b2f5289463503))
* **session:** prevent project loss during window restore ([#103](https://github.com/itsoltech/canopy-desktop/issues/103)) ([6f75775](https://github.com/itsoltech/canopy-desktop/commit/6f757758ef62779490ff7f006ce44a4f4b6c09f0))
* **session:** reliably persist window configs on quit ([#106](https://github.com/itsoltech/canopy-desktop/issues/106)) ([e96ba59](https://github.com/itsoltech/canopy-desktop/commit/e96ba595ba907d9478e8990b8badb834e78d15a3))
* **session:** restore all workspaces and clean up layouts on close ([10f058a](https://github.com/itsoltech/canopy-desktop/commit/10f058a79a416752ef13aa144bb2bc99897912ae))
* **tasks:** run worktree setup commands when starting task ([#102](https://github.com/itsoltech/canopy-desktop/issues/102)) ([b218242](https://github.com/itsoltech/canopy-desktop/commit/b218242008d3f685e96b2cd4c89f00b4ce3067ac))
* **terminal:** prevent terminal content from overflowing viewport ([de22b78](https://github.com/itsoltech/canopy-desktop/commit/de22b788523c2b970406962ba8449c7617714937))
* **titlebar:** prevent text wrap on narrow windows ([b0d0bc7](https://github.com/itsoltech/canopy-desktop/commit/b0d0bc7d33f50809510b598ddd30ac97b811f987))
* **ui:** align Gemini prefs, keyboard symbols, and diff comment UX ([#124](https://github.com/itsoltech/canopy-desktop/issues/124)) ([71b3ec1](https://github.com/itsoltech/canopy-desktop/commit/71b3ec1845882ab83af10bb6f0c5d7a635028131))
* **ui:** improve PR approved badge contrast in sidebar ([81bba04](https://github.com/itsoltech/canopy-desktop/commit/81bba04385626affd9ca4b7c89bf29b714128fe1))

### Refactoring

* cross-platform file manager labels and path handling ([#98](https://github.com/itsoltech/canopy-desktop/issues/98)) ([aa6cda3](https://github.com/itsoltech/canopy-desktop/commit/aa6cda3954f2f4f3bc18c79564fb61ccf93755cc))
* group settings tabs into thematic categories ([#99](https://github.com/itsoltech/canopy-desktop/issues/99)) ([4258437](https://github.com/itsoltech/canopy-desktop/commit/4258437d7bd59bdb6ffcae64913043f6d05e6bc0))
* replace try/catch with neverthrow typed error handling ([#91](https://github.com/itsoltech/canopy-desktop/issues/91)) ([5612434](https://github.com/itsoltech/canopy-desktop/commit/561243412379aa5d8f28e36c13e68d1830a9e955))

## [0.10.0-next.11](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.10...v0.10.0-next.11) (2026-04-09)

### Features

* **worktree:** create worktree from existing branch ([#125](https://github.com/itsoltech/canopy-desktop/issues/125)) ([1b933dd](https://github.com/itsoltech/canopy-desktop/commit/1b933dd67eb4b5918923eae10ae39bdd508af261))

### Bug Fixes

* **titlebar:** prevent text wrap on narrow windows ([b0d0bc7](https://github.com/itsoltech/canopy-desktop/commit/b0d0bc7d33f50809510b598ddd30ac97b811f987))
* **ui:** align Gemini prefs, keyboard symbols, and diff comment UX ([#124](https://github.com/itsoltech/canopy-desktop/issues/124)) ([71b3ec1](https://github.com/itsoltech/canopy-desktop/commit/71b3ec1845882ab83af10bb6f0c5d7a635028131))

## [0.10.0-next.10](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.9...v0.10.0-next.10) (2026-04-09)

### Features

* **ci:** auto-track claude code releases for compatibility ([#121](https://github.com/itsoltech/canopy-desktop/issues/121)) ([338d5a2](https://github.com/itsoltech/canopy-desktop/commit/338d5a209eb513d91fc2b279d75542b4acfd7c33))
* **taskTracker:** restore dual-layer config and multi-tracker support ([#118](https://github.com/itsoltech/canopy-desktop/issues/118)) ([30a28ff](https://github.com/itsoltech/canopy-desktop/commit/30a28ff2964ebf09ad7f3c8805cf5052e3c4e03f)), closes [#94](https://github.com/itsoltech/canopy-desktop/issues/94) [#94](https://github.com/itsoltech/canopy-desktop/issues/94)
* **updater:** configurable auto-update check frequency ([#122](https://github.com/itsoltech/canopy-desktop/issues/122)) ([62aced6](https://github.com/itsoltech/canopy-desktop/commit/62aced67207eb362d49c3d748640588297290973))
* **window:** include active tab name in window title ([#116](https://github.com/itsoltech/canopy-desktop/issues/116)) ([5f29b71](https://github.com/itsoltech/canopy-desktop/commit/5f29b71c4af9b02ad2082c3a4ca2936eaf44c6f3))

### Bug Fixes

* **git,ui:** resilient git detection and full-width preference selects ([#117](https://github.com/itsoltech/canopy-desktop/issues/117)) ([49d0d7c](https://github.com/itsoltech/canopy-desktop/commit/49d0d7c030c3b406cdb4b6c84d1dcc28cd31ff06))

## [0.10.0-next.9](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.8...v0.10.0-next.9) (2026-04-08)

### Features

* **sidebar:** dynamic file tree via native filesystem watcher ([#114](https://github.com/itsoltech/canopy-desktop/issues/114)) ([4a48626](https://github.com/itsoltech/canopy-desktop/commit/4a4862616c30844552fb03fd9227e573c031d120))

### Bug Fixes

* **diff:** sticky header, untracked files, scroll and layout issues ([#113](https://github.com/itsoltech/canopy-desktop/issues/113)) ([a8d3314](https://github.com/itsoltech/canopy-desktop/commit/a8d33140abdfaa0ccc9cd316d0cdcba7d1007f75))

## [0.10.0-next.8](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.7...v0.10.0-next.8) (2026-04-07)

### Features

* **telemetry:** add daily usage ping via Umami with opt-out in settings ([612cefd](https://github.com/itsoltech/canopy-desktop/commit/612cefd0844ad5c399099090a899352c7a5ee152))
* **window:** restore window position, size, and state across restarts ([#111](https://github.com/itsoltech/canopy-desktop/issues/111)) ([1cac80e](https://github.com/itsoltech/canopy-desktop/commit/1cac80e5288d09927bacd061be34012d48f7a535))

### Bug Fixes

* **git:** sanitize branch and worktree names to strip unsafe characters ([#110](https://github.com/itsoltech/canopy-desktop/issues/110)) ([c3822f4](https://github.com/itsoltech/canopy-desktop/commit/c3822f42ca8e64ebb228c8be336b2f5289463503))

## [0.10.0-next.7](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.6...v0.10.0-next.7) (2026-04-07)

### Bug Fixes

* **diff:** prevent layout jump when clicking refresh button ([#107](https://github.com/itsoltech/canopy-desktop/issues/107)) ([e9a7ed0](https://github.com/itsoltech/canopy-desktop/commit/e9a7ed075ce1fee067c3eb53dad30e14cc696ab6))
* **git:** refresh worktree list on branch checkout ([#109](https://github.com/itsoltech/canopy-desktop/issues/109)) ([a9604af](https://github.com/itsoltech/canopy-desktop/commit/a9604af636ca8aff61c9b82261db9f64cd51fd63)), closes [#108](https://github.com/itsoltech/canopy-desktop/issues/108)

## [0.10.0-next.6](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.5...v0.10.0-next.6) (2026-04-07)

### Bug Fixes

* **session:** restore all workspaces and clean up layouts on close ([10f058a](https://github.com/itsoltech/canopy-desktop/commit/10f058a79a416752ef13aa144bb2bc99897912ae))
* **ui:** improve PR approved badge contrast in sidebar ([81bba04](https://github.com/itsoltech/canopy-desktop/commit/81bba04385626affd9ca4b7c89bf29b714128fe1))

## [0.10.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.4...v0.10.0-next.5) (2026-04-07)

### Bug Fixes

* **diff:** replace emoji icons, fix tab flash, comment form polish ([#105](https://github.com/itsoltech/canopy-desktop/issues/105)) ([211e165](https://github.com/itsoltech/canopy-desktop/commit/211e16525037e651d5bb67ce5ee8ebdc5eaa6096))
* **session:** reliably persist window configs on quit ([#106](https://github.com/itsoltech/canopy-desktop/issues/106)) ([e96ba59](https://github.com/itsoltech/canopy-desktop/commit/e96ba595ba907d9478e8990b8badb834e78d15a3))

## [0.10.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.3...v0.10.0-next.4) (2026-04-07)

### Bug Fixes

* **session:** prevent project loss during window restore ([#103](https://github.com/itsoltech/canopy-desktop/issues/103)) ([6f75775](https://github.com/itsoltech/canopy-desktop/commit/6f757758ef62779490ff7f006ce44a4f4b6c09f0))
* **tasks:** run worktree setup commands when starting task ([#102](https://github.com/itsoltech/canopy-desktop/issues/102)) ([b218242](https://github.com/itsoltech/canopy-desktop/commit/b218242008d3f685e96b2cd4c89f00b4ce3067ac))

### Refactoring

* cross-platform file manager labels and path handling ([#98](https://github.com/itsoltech/canopy-desktop/issues/98)) ([aa6cda3](https://github.com/itsoltech/canopy-desktop/commit/aa6cda3954f2f4f3bc18c79564fb61ccf93755cc))
* group settings tabs into thematic categories ([#99](https://github.com/itsoltech/canopy-desktop/issues/99)) ([4258437](https://github.com/itsoltech/canopy-desktop/commit/4258437d7bd59bdb6ffcae64913043f6d05e6bc0))

## [0.10.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.2...v0.10.0-next.3) (2026-04-05)

### Features

* **tasks:** repo-level config, keychain credentials, PR workflow ([#94](https://github.com/itsoltech/canopy-desktop/issues/94)) ([14c513f](https://github.com/itsoltech/canopy-desktop/commit/14c513f662325c22c32aad84a3239c9f158631ed))

## [0.10.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.10.0-next.1...v0.10.0-next.2) (2026-04-04)

### Features

- add diff review panel with tabbed right sidebar ([#96](https://github.com/itsoltech/canopy-desktop/issues/96)) ([8945a44](https://github.com/itsoltech/canopy-desktop/commit/8945a447b7b0daa179a3c7e9361b3123d4743abe))
- add keystroke visualizer overlay with WPM tracking ([#59](https://github.com/itsoltech/canopy-desktop/issues/59)) ([5bd0c5d](https://github.com/itsoltech/canopy-desktop/commit/5bd0c5d25738f5da1d452585d686d02af6ec5316))
- add performance testing infrastructure ([#92](https://github.com/itsoltech/canopy-desktop/issues/92)) ([4df7adc](https://github.com/itsoltech/canopy-desktop/commit/4df7adcdb9a357e18261b8f09f3a48857699ffd2))
- **github:** add GitHub integration with GraphQL API ([#97](https://github.com/itsoltech/canopy-desktop/issues/97)) ([36626f8](https://github.com/itsoltech/canopy-desktop/commit/36626f8164d6365600bba077b4b76215c1b8a173))

### Bug Fixes

- **terminal:** prevent terminal content from overflowing viewport ([de22b78](https://github.com/itsoltech/canopy-desktop/commit/de22b788523c2b970406962ba8449c7617714937))

## [0.10.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0...v0.10.0-next.1) (2026-04-03)

### Features

- **tasks:** send task context to agent sessions ([#90](https://github.com/itsoltech/canopy-desktop/issues/90)) ([7f05068](https://github.com/itsoltech/canopy-desktop/commit/7f05068316538421d649a6821bde47bb71041f98))

### Refactoring

- replace try/catch with neverthrow typed error handling ([#91](https://github.com/itsoltech/canopy-desktop/issues/91)) ([5612434](https://github.com/itsoltech/canopy-desktop/commit/561243412379aa5d8f28e36c13e68d1830a9e955))

## [0.9.0](https://github.com/itsoltech/canopy-desktop/compare/v0.8.1...v0.9.0) (2026-04-03)

### Features

- configurable startup tool for new tabs and worktrees ([#85](https://github.com/itsoltech/canopy-desktop/issues/85)) ([b90377c](https://github.com/itsoltech/canopy-desktop/commit/b90377cbc8d14c1074b4a431fd686e1c1e0e91cc))
- **onboarding:** add tool selection and environment check steps ([#89](https://github.com/itsoltech/canopy-desktop/issues/89)) ([44ff8f3](https://github.com/itsoltech/canopy-desktop/commit/44ff8f3c918b3c6c898e16d8421be8645c76b547))
- **session:** lazy restore tabs on startup ([#72](https://github.com/itsoltech/canopy-desktop/issues/72)) ([058fe87](https://github.com/itsoltech/canopy-desktop/commit/058fe878072d058f291707677e22270fba5bdc4d))
- **tasks:** persist last selected board in task picker ([#88](https://github.com/itsoltech/canopy-desktop/issues/88)) ([8828516](https://github.com/itsoltech/canopy-desktop/commit/8828516bcb8467f06f03fb456744a92e7c03b82f))

### Bug Fixes

- code review corrections across terminal and task tracker ([#80](https://github.com/itsoltech/canopy-desktop/issues/80)) ([8277373](https://github.com/itsoltech/canopy-desktop/commit/82773734b37fe70145783d19e159522cc2a66d86))
- **perf:** selective git refresh, shared WS server, terminal fixes ([#77](https://github.com/itsoltech/canopy-desktop/issues/77)) ([b77d0fd](https://github.com/itsoltech/canopy-desktop/commit/b77d0fdbddd4bb985aeb57743210146818b6edfc))
- **session:** clean up orphaned tmux sessions and restore lifecycle ([#74](https://github.com/itsoltech/canopy-desktop/issues/74)) ([76914a5](https://github.com/itsoltech/canopy-desktop/commit/76914a5f48561d7f72dab0f970ef7e53f4044672))
- **terminal:** block Ctrl+Z in AI terminals to prevent suspend ([#71](https://github.com/itsoltech/canopy-desktop/issues/71)) ([ca4d6c1](https://github.com/itsoltech/canopy-desktop/commit/ca4d6c1ea15ff9c990e5222e161ce914f9278c62))
- **terminal:** fix Shift+Enter inserting newline in all terminals ([#82](https://github.com/itsoltech/canopy-desktop/issues/82)) ([86db02c](https://github.com/itsoltech/canopy-desktop/commit/86db02c9558a1f794498089b3f5f828d286e3cc7)), closes [#81](https://github.com/itsoltech/canopy-desktop/issues/81)
- **terminal:** forward process titles through tmux to tab names ([#76](https://github.com/itsoltech/canopy-desktop/issues/76)) ([05dbbdd](https://github.com/itsoltech/canopy-desktop/commit/05dbbdd47f9fb2e5d8ad3690c2ef5a1475021bcf))
- **terminal:** isolate tmux socket between dev and prod runs ([972c4be](https://github.com/itsoltech/canopy-desktop/commit/972c4beb6678e852a56bbbcf357c2d872919051b))
- **terminal:** pass agent env vars to tmux sessions ([#73](https://github.com/itsoltech/canopy-desktop/issues/73)) ([e46e44a](https://github.com/itsoltech/canopy-desktop/commit/e46e44ae086e00c97a4b3097c50c33b21f96634b))
- unblock dependabot PR automation ([#78](https://github.com/itsoltech/canopy-desktop/issues/78)) ([90b2ccb](https://github.com/itsoltech/canopy-desktop/commit/90b2ccbb6d3f201bab30a4f927ae5b5e7f458ec9)), closes [#75](https://github.com/itsoltech/canopy-desktop/issues/75)

### Refactoring

- adopt ts-pattern for exhaustive pattern matching ([#83](https://github.com/itsoltech/canopy-desktop/issues/83)) ([4e0b185](https://github.com/itsoltech/canopy-desktop/commit/4e0b18551c6c4ff1fc5546f65caa850bae1875d3))

## [0.9.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0-next.4...v0.9.0-next.5) (2026-04-03)

### Features

- configurable startup tool for new tabs and worktrees ([#85](https://github.com/itsoltech/canopy-desktop/issues/85)) ([b90377c](https://github.com/itsoltech/canopy-desktop/commit/b90377cbc8d14c1074b4a431fd686e1c1e0e91cc))
- **tasks:** persist last selected board in task picker ([#88](https://github.com/itsoltech/canopy-desktop/issues/88)) ([8828516](https://github.com/itsoltech/canopy-desktop/commit/8828516bcb8467f06f03fb456744a92e7c03b82f))

### Bug Fixes

- code review corrections across terminal and task tracker ([#80](https://github.com/itsoltech/canopy-desktop/issues/80)) ([8277373](https://github.com/itsoltech/canopy-desktop/commit/82773734b37fe70145783d19e159522cc2a66d86))
- **terminal:** fix Shift+Enter inserting newline in all terminals ([#82](https://github.com/itsoltech/canopy-desktop/issues/82)) ([86db02c](https://github.com/itsoltech/canopy-desktop/commit/86db02c9558a1f794498089b3f5f828d286e3cc7)), closes [#81](https://github.com/itsoltech/canopy-desktop/issues/81)

### Refactoring

- adopt ts-pattern for exhaustive pattern matching ([#83](https://github.com/itsoltech/canopy-desktop/issues/83)) ([4e0b185](https://github.com/itsoltech/canopy-desktop/commit/4e0b18551c6c4ff1fc5546f65caa850bae1875d3))

## [0.9.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0-next.3...v0.9.0-next.4) (2026-04-03)

### Bug Fixes

- unblock dependabot PR automation ([#78](https://github.com/itsoltech/canopy-desktop/issues/78)) ([90b2ccb](https://github.com/itsoltech/canopy-desktop/commit/90b2ccbb6d3f201bab30a4f927ae5b5e7f458ec9)), closes [#75](https://github.com/itsoltech/canopy-desktop/issues/75)

## [0.9.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0-next.2...v0.9.0-next.3) (2026-04-02)

### Bug Fixes

- **perf:** selective git refresh, shared WS server, terminal fixes ([#77](https://github.com/itsoltech/canopy-desktop/issues/77)) ([b77d0fd](https://github.com/itsoltech/canopy-desktop/commit/b77d0fdbddd4bb985aeb57743210146818b6edfc))

## [0.9.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.9.0-next.1...v0.9.0-next.2) (2026-04-02)

### Bug Fixes

- **session:** clean up orphaned tmux sessions and restore lifecycle ([#74](https://github.com/itsoltech/canopy-desktop/issues/74)) ([76914a5](https://github.com/itsoltech/canopy-desktop/commit/76914a5f48561d7f72dab0f970ef7e53f4044672))
- **terminal:** forward process titles through tmux to tab names ([#76](https://github.com/itsoltech/canopy-desktop/issues/76)) ([05dbbdd](https://github.com/itsoltech/canopy-desktop/commit/05dbbdd47f9fb2e5d8ad3690c2ef5a1475021bcf))
- **terminal:** isolate tmux socket between dev and prod runs ([972c4be](https://github.com/itsoltech/canopy-desktop/commit/972c4beb6678e852a56bbbcf357c2d872919051b))
- **terminal:** pass agent env vars to tmux sessions ([#73](https://github.com/itsoltech/canopy-desktop/issues/73)) ([e46e44a](https://github.com/itsoltech/canopy-desktop/commit/e46e44ae086e00c97a4b3097c50c33b21f96634b))

## [0.9.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.8.1...v0.9.0-next.1) (2026-04-02)

### Features

- **session:** lazy restore tabs on startup ([#72](https://github.com/itsoltech/canopy-desktop/issues/72)) ([058fe87](https://github.com/itsoltech/canopy-desktop/commit/058fe878072d058f291707677e22270fba5bdc4d))

### Bug Fixes

- **terminal:** block Ctrl+Z in AI terminals to prevent suspend ([#71](https://github.com/itsoltech/canopy-desktop/issues/71)) ([ca4d6c1](https://github.com/itsoltech/canopy-desktop/commit/ca4d6c1ea15ff9c990e5222e161ce914f9278c62))

## [0.8.1](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0...v0.8.1) (2026-04-01)

### Bug Fixes

- **terminal:** prevent WebGL crash on session restore ([#70](https://github.com/itsoltech/canopy-desktop/issues/70)) ([d321a69](https://github.com/itsoltech/canopy-desktop/commit/d321a69ba815cce81f6c2a47abe0a2149748df68))

## [0.8.0](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0...v0.8.0) (2026-04-01)

### Features

- add task tracker integrations (Jira & YouTrack) ([#66](https://github.com/itsoltech/canopy-desktop/issues/66)) ([206d7b6](https://github.com/itsoltech/canopy-desktop/commit/206d7b6049620dfa3b0f90cd16bf517134cc9b18))
- add user onboarding system ([#58](https://github.com/itsoltech/canopy-desktop/issues/58)) ([b5536a4](https://github.com/itsoltech/canopy-desktop/commit/b5536a4f880e2af4cd0615ed799517c344c44019))
- **browser:** migrate to webview, add credentials, favorites, viewports, and device emulation ([#55](https://github.com/itsoltech/canopy-desktop/issues/55)) ([8a01f52](https://github.com/itsoltech/canopy-desktop/commit/8a01f52277bc196191fade3ca1cc517908d6c27e))
- custom tool editing and live refresh ([#65](https://github.com/itsoltech/canopy-desktop/issues/65)) ([2019720](https://github.com/itsoltech/canopy-desktop/commit/201972076470244745dfcf73ddb86ebb46abe492))
- **inspector:** generic agent abstraction with Gemini CLI support ([#33](https://github.com/itsoltech/canopy-desktop/issues/33)) ([0b6e6f1](https://github.com/itsoltech/canopy-desktop/commit/0b6e6f1289def57f25a2c0efab7113cb4d07f16b))
- **pty:** truecolor support for terminals ([#34](https://github.com/itsoltech/canopy-desktop/issues/34)) ([9de8f60](https://github.com/itsoltech/canopy-desktop/commit/9de8f60c064bd6cb82e34c707a3017b3c06ca394))
- **sidebar:** add "New Worktree from Branch" to context menu ([#35](https://github.com/itsoltech/canopy-desktop/issues/35)) ([95c4615](https://github.com/itsoltech/canopy-desktop/commit/95c4615a8715d9dc6975e257635d4843db9aaafe))
- **sidebar:** show loading indicator when removing worktree ([#38](https://github.com/itsoltech/canopy-desktop/issues/38)) ([d5e74d6](https://github.com/itsoltech/canopy-desktop/commit/d5e74d62697b9c51413ad9ab736942505bff43f4))
- **statusbar:** add bottom status bar ([#51](https://github.com/itsoltech/canopy-desktop/issues/51)) ([5d69f33](https://github.com/itsoltech/canopy-desktop/commit/5d69f33fda482c6483684b508331375849bcba61))
- sync app UI theme with terminal theme ([#57](https://github.com/itsoltech/canopy-desktop/issues/57)) ([3eba336](https://github.com/itsoltech/canopy-desktop/commit/3eba336a63b4e0277b0e7b1a126a3745f71d71ba))
- **terminal:** add tmux session persistence for shell sessions ([#64](https://github.com/itsoltech/canopy-desktop/issues/64)) ([c33704d](https://github.com/itsoltech/canopy-desktop/commit/c33704d89a9ff26aa5cd3856c26171068b70daf1))
- **terminal:** add typing speed (WPM) tracker ([#53](https://github.com/itsoltech/canopy-desktop/issues/53)) ([5cc5b0a](https://github.com/itsoltech/canopy-desktop/commit/5cc5b0acb52f6f975f6dc9f0dc99239ee25c137a))
- **terminal:** extend tmux to all tools and add mouse support ([#67](https://github.com/itsoltech/canopy-desktop/issues/67)) ([251379c](https://github.com/itsoltech/canopy-desktop/commit/251379ce6cb1010e318561cd27c590f03f5c80d4))
- **worktree:** stream setup command output in terminal ([#52](https://github.com/itsoltech/canopy-desktop/issues/52)) ([6fa5984](https://github.com/itsoltech/canopy-desktop/commit/6fa5984378e0420a4881a69cee9a23899529073f))

### Bug Fixes

- **browser:** hide devtools for overlays and unify inspect element flow ([#63](https://github.com/itsoltech/canopy-desktop/issues/63)) ([78d3a49](https://github.com/itsoltech/canopy-desktop/commit/78d3a49a41bd89e21c0b764146c17c3ed0080f36))
- **browser:** hide devtools view when tab inactive or modal open ([#62](https://github.com/itsoltech/canopy-desktop/issues/62)) ([e5cc762](https://github.com/itsoltech/canopy-desktop/commit/e5cc7626ed984b8588ee97622a89a7d83a0f0a2e))
- **ci:** add ANTHROPIC_AUTH_TOKEN env var to all Claude workflows ([0cd9527](https://github.com/itsoltech/canopy-desktop/commit/0cd9527f5220af3e79ebab67b8792e41e93baf87))
- **file-tree:** include active worktree in path validation ([#50](https://github.com/itsoltech/canopy-desktop/issues/50)) ([c28ea44](https://github.com/itsoltech/canopy-desktop/commit/c28ea44e21946a3e0d8972847651e2912d88f6d6))
- **modals:** auto-focus and prevent close on text selection drag ([#39](https://github.com/itsoltech/canopy-desktop/issues/39)) ([f70cdc0](https://github.com/itsoltech/canopy-desktop/commit/f70cdc0d64d86e45e4af6cf0fdd2e05f45c364b9))
- **notch:** enable notch overlay and hook scripts on Windows ([#60](https://github.com/itsoltech/canopy-desktop/issues/60)) ([072bc78](https://github.com/itsoltech/canopy-desktop/commit/072bc78cb147b0e1f9568eee962f20534b748cb1))
- **notch:** restore Dock icon after panel window hides it ([#54](https://github.com/itsoltech/canopy-desktop/issues/54)) ([dc4cc3d](https://github.com/itsoltech/canopy-desktop/commit/dc4cc3d10d259bab0f89faa8b1e989cecb77ed45))
- **terminal:** match container background with terminal theme ([6bf60ad](https://github.com/itsoltech/canopy-desktop/commit/6bf60adec8e167785ae8d52f6b2a9e9d1f752c44))
- **terminal:** reduce renderer memory usage ([#32](https://github.com/itsoltech/canopy-desktop/issues/32)) ([f534c78](https://github.com/itsoltech/canopy-desktop/commit/f534c78e23bfc755406cab993781ae354cd1cacf))
- **terminal:** restore focus after file drop and screenshot delivery ([#37](https://github.com/itsoltech/canopy-desktop/issues/37)) ([3c46743](https://github.com/itsoltech/canopy-desktop/commit/3c4674359605a547ec4ed673b932fbe3e78280dc))
- **updater:** include stable releases in pre-release update channel ([#36](https://github.com/itsoltech/canopy-desktop/issues/36)) ([098d70e](https://github.com/itsoltech/canopy-desktop/commit/098d70e4b6935dba02eec1039a8cdc2d9a719510))

### Refactoring

- migrate commands to skills architecture and add self-review ([85803d0](https://github.com/itsoltech/canopy-desktop/commit/85803d081073a6054dd5651b3aa38ef94c9bbd1c))
- **ui:** replace native forms with custom components ([#68](https://github.com/itsoltech/canopy-desktop/issues/68)) ([23f1d9f](https://github.com/itsoltech/canopy-desktop/commit/23f1d9f20b8c8e8afdcc8f383cde4f3b1388d2e2))

## [0.8.0-next.8](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.7...v0.8.0-next.8) (2026-04-01)

### Features

- **terminal:** extend tmux to all tools and add mouse support ([#67](https://github.com/itsoltech/canopy-desktop/issues/67)) ([251379c](https://github.com/itsoltech/canopy-desktop/commit/251379ce6cb1010e318561cd27c590f03f5c80d4))

## [0.8.0-next.7](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.6...v0.8.0-next.7) (2026-04-01)

### Features

- **terminal:** add tmux session persistence for shell sessions ([#64](https://github.com/itsoltech/canopy-desktop/issues/64)) ([c33704d](https://github.com/itsoltech/canopy-desktop/commit/c33704d89a9ff26aa5cd3856c26171068b70daf1))

## [0.8.0-next.6](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.5...v0.8.0-next.6) (2026-04-01)

### Features

- add task tracker integrations (Jira & YouTrack) ([#66](https://github.com/itsoltech/canopy-desktop/issues/66)) ([206d7b6](https://github.com/itsoltech/canopy-desktop/commit/206d7b6049620dfa3b0f90cd16bf517134cc9b18))

## [0.8.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.4...v0.8.0-next.5) (2026-04-01)

### Features

- custom tool editing and live refresh ([#65](https://github.com/itsoltech/canopy-desktop/issues/65)) ([2019720](https://github.com/itsoltech/canopy-desktop/commit/201972076470244745dfcf73ddb86ebb46abe492))

### Bug Fixes

- **browser:** hide devtools for overlays and unify inspect element flow ([#63](https://github.com/itsoltech/canopy-desktop/issues/63)) ([78d3a49](https://github.com/itsoltech/canopy-desktop/commit/78d3a49a41bd89e21c0b764146c17c3ed0080f36))

## [0.8.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.3...v0.8.0-next.4) (2026-04-01)

### Bug Fixes

- **browser:** hide devtools view when tab inactive or modal open ([#62](https://github.com/itsoltech/canopy-desktop/issues/62)) ([e5cc762](https://github.com/itsoltech/canopy-desktop/commit/e5cc7626ed984b8588ee97622a89a7d83a0f0a2e))
- **ci:** add ANTHROPIC_AUTH_TOKEN env var to all Claude workflows ([0cd9527](https://github.com/itsoltech/canopy-desktop/commit/0cd9527f5220af3e79ebab67b8792e41e93baf87))
- **notch:** enable notch overlay and hook scripts on Windows ([#60](https://github.com/itsoltech/canopy-desktop/issues/60)) ([072bc78](https://github.com/itsoltech/canopy-desktop/commit/072bc78cb147b0e1f9568eee962f20534b748cb1))
- **terminal:** match container background with terminal theme ([6bf60ad](https://github.com/itsoltech/canopy-desktop/commit/6bf60adec8e167785ae8d52f6b2a9e9d1f752c44))

## [0.8.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.2...v0.8.0-next.3) (2026-03-31)

### Features

- add user onboarding system ([#58](https://github.com/itsoltech/canopy-desktop/issues/58)) ([b5536a4](https://github.com/itsoltech/canopy-desktop/commit/b5536a4f880e2af4cd0615ed799517c344c44019))
- **browser:** migrate to webview, add credentials, favorites, viewports, and device emulation ([#55](https://github.com/itsoltech/canopy-desktop/issues/55)) ([8a01f52](https://github.com/itsoltech/canopy-desktop/commit/8a01f52277bc196191fade3ca1cc517908d6c27e))
- sync app UI theme with terminal theme ([#57](https://github.com/itsoltech/canopy-desktop/issues/57)) ([3eba336](https://github.com/itsoltech/canopy-desktop/commit/3eba336a63b4e0277b0e7b1a126a3745f71d71ba))
- **worktree:** stream setup command output in terminal ([#52](https://github.com/itsoltech/canopy-desktop/issues/52)) ([6fa5984](https://github.com/itsoltech/canopy-desktop/commit/6fa5984378e0420a4881a69cee9a23899529073f))

### Bug Fixes

- **notch:** restore Dock icon after panel window hides it ([#54](https://github.com/itsoltech/canopy-desktop/issues/54)) ([dc4cc3d](https://github.com/itsoltech/canopy-desktop/commit/dc4cc3d10d259bab0f89faa8b1e989cecb77ed45))

### Refactoring

- migrate commands to skills architecture and add self-review ([85803d0](https://github.com/itsoltech/canopy-desktop/commit/85803d081073a6054dd5651b3aa38ef94c9bbd1c))

## [0.8.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.8.0-next.1...v0.8.0-next.2) (2026-03-31)

### Features

- **statusbar:** add bottom status bar ([#51](https://github.com/itsoltech/canopy-desktop/issues/51)) ([5d69f33](https://github.com/itsoltech/canopy-desktop/commit/5d69f33fda482c6483684b508331375849bcba61))
- **terminal:** add typing speed (WPM) tracker ([#53](https://github.com/itsoltech/canopy-desktop/issues/53)) ([5cc5b0a](https://github.com/itsoltech/canopy-desktop/commit/5cc5b0acb52f6f975f6dc9f0dc99239ee25c137a))

### Bug Fixes

- **file-tree:** include active worktree in path validation ([#50](https://github.com/itsoltech/canopy-desktop/issues/50)) ([c28ea44](https://github.com/itsoltech/canopy-desktop/commit/c28ea44e21946a3e0d8972847651e2912d88f6d6))
- **modals:** auto-focus and prevent close on text selection drag ([#39](https://github.com/itsoltech/canopy-desktop/issues/39)) ([f70cdc0](https://github.com/itsoltech/canopy-desktop/commit/f70cdc0d64d86e45e4af6cf0fdd2e05f45c364b9))

## [0.8.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.7.1-next.1...v0.8.0-next.1) (2026-03-30)

### Features

- **inspector:** generic agent abstraction with Gemini CLI support ([#33](https://github.com/itsoltech/canopy-desktop/issues/33)) ([0b6e6f1](https://github.com/itsoltech/canopy-desktop/commit/0b6e6f1289def57f25a2c0efab7113cb4d07f16b))
- **pty:** truecolor support for terminals ([#34](https://github.com/itsoltech/canopy-desktop/issues/34)) ([9de8f60](https://github.com/itsoltech/canopy-desktop/commit/9de8f60c064bd6cb82e34c707a3017b3c06ca394))
- **sidebar:** add "New Worktree from Branch" to context menu ([#35](https://github.com/itsoltech/canopy-desktop/issues/35)) ([95c4615](https://github.com/itsoltech/canopy-desktop/commit/95c4615a8715d9dc6975e257635d4843db9aaafe))
- **sidebar:** show loading indicator when removing worktree ([#38](https://github.com/itsoltech/canopy-desktop/issues/38)) ([d5e74d6](https://github.com/itsoltech/canopy-desktop/commit/d5e74d62697b9c51413ad9ab736942505bff43f4))

### Bug Fixes

- **terminal:** restore focus after file drop and screenshot delivery ([#37](https://github.com/itsoltech/canopy-desktop/issues/37)) ([3c46743](https://github.com/itsoltech/canopy-desktop/commit/3c4674359605a547ec4ed673b932fbe3e78280dc))
- **updater:** include stable releases in pre-release update channel ([#36](https://github.com/itsoltech/canopy-desktop/issues/36)) ([098d70e](https://github.com/itsoltech/canopy-desktop/commit/098d70e4b6935dba02eec1039a8cdc2d9a719510))

## [0.7.1-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0...v0.7.1-next.1) (2026-03-30)

### Bug Fixes

- **terminal:** reduce renderer memory usage ([#32](https://github.com/itsoltech/canopy-desktop/issues/32)) ([f534c78](https://github.com/itsoltech/canopy-desktop/commit/f534c78e23bfc755406cab993781ae354cd1cacf))

## [0.7.0](https://github.com/itsoltech/canopy-desktop/compare/v0.6.2...v0.7.0) (2026-03-30)

### Features

- add file browser with read-only editor pane ([#18](https://github.com/itsoltech/canopy-desktop/issues/18)) ([9b5fb27](https://github.com/itsoltech/canopy-desktop/commit/9b5fb275b42c74f120bf8bb8f021676cb40a1440))
- next branch pre-release pipeline and update channel settings ([#19](https://github.com/itsoltech/canopy-desktop/issues/19)) ([528b0c5](https://github.com/itsoltech/canopy-desktop/commit/528b0c599cb0354a43500fbf377ed053e224f877))
- post-update changelog modal with release notes ([#22](https://github.com/itsoltech/canopy-desktop/issues/22)) ([2575ef6](https://github.com/itsoltech/canopy-desktop/commit/2575ef69d2cc6e21d165163efb94529426eb5e11))
- **sidebar:** agent status indicators on worktree items ([#25](https://github.com/itsoltech/canopy-desktop/issues/25)) ([52f8238](https://github.com/itsoltech/canopy-desktop/commit/52f82386c86e496c9de1e6284fb9ee0758d59216))
- **sidebar:** draggable resize handle ([#30](https://github.com/itsoltech/canopy-desktop/issues/30)) ([6eac17b](https://github.com/itsoltech/canopy-desktop/commit/6eac17bc0f2b6b01227a4fdfa0817226fd5155f0))
- **sidebar:** section visibility and ordering settings ([#28](https://github.com/itsoltech/canopy-desktop/issues/28)) ([e69c684](https://github.com/itsoltech/canopy-desktop/commit/e69c684d6f30aff627b8af630ff5c227b32edab2))

### Bug Fixes

- **ci:** add --auto flag to auto-merge gh pr merge command ([2028541](https://github.com/itsoltech/canopy-desktop/commit/20285418dd83c411141553bbe179fea8420364d9))
- **ci:** correct required label name to claude:review:approved ([c5f1758](https://github.com/itsoltech/canopy-desktop/commit/c5f1758b1d44e954c6c1db6847ee013b1f5cc5ac))
- **ci:** use PAT for semantic-release to bypass branch ruleset ([4c367e2](https://github.com/itsoltech/canopy-desktop/commit/4c367e2bca7a58ec4b11afb59833f5c36c1095fc))
- **menu:** add Preferences item to macOS app menu ([#29](https://github.com/itsoltech/canopy-desktop/issues/29)) ([a5d9344](https://github.com/itsoltech/canopy-desktop/commit/a5d9344a166c793a697c36a8e7bd19eb566aa983))
- **notch:** disable text selection in notch overlay ([a2b0e56](https://github.com/itsoltech/canopy-desktop/commit/a2b0e56f780c0af33a7932a33b6003147549658f))
- **notch:** remove 5-item cap and fix height calculation in agent list ([#24](https://github.com/itsoltech/canopy-desktop/issues/24)) ([224595f](https://github.com/itsoltech/canopy-desktop/commit/224595f48454ae856b081241fe3d4b25289040e7))
- **terminal:** preserve scroll position when new content arrives ([#26](https://github.com/itsoltech/canopy-desktop/issues/26)) ([7507315](https://github.com/itsoltech/canopy-desktop/commit/75073154541f69fbea0284497add5c72a273a8ca))
- **updater:** bypass unreliable quitAndInstall on macOS ([#27](https://github.com/itsoltech/canopy-desktop/issues/27)) ([ff2ce6f](https://github.com/itsoltech/canopy-desktop/commit/ff2ce6f0cf12eef6ac2c48617ad2ac55343d7ab6))
- **updater:** use quitAndInstall on macOS instead of app.relaunch ([7e441cc](https://github.com/itsoltech/canopy-desktop/commit/7e441cc5708ca2689d10eebebf95ae770321c0b4))

### Refactoring

- **sidebar:** remove file tree section from sidebar ([c24b1da](https://github.com/itsoltech/canopy-desktop/commit/c24b1da031c4b877893b88d3d62ccfd581fa800f))

## [0.7.0-next.10](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.9...v0.7.0-next.10) (2026-03-30)

### Features

- **sidebar:** draggable resize handle ([#30](https://github.com/itsoltech/canopy-desktop/issues/30)) ([6eac17b](https://github.com/itsoltech/canopy-desktop/commit/6eac17bc0f2b6b01227a4fdfa0817226fd5155f0))

### Bug Fixes

- **menu:** add Preferences item to macOS app menu ([#29](https://github.com/itsoltech/canopy-desktop/issues/29)) ([a5d9344](https://github.com/itsoltech/canopy-desktop/commit/a5d9344a166c793a697c36a8e7bd19eb566aa983))

## [0.7.0-next.9](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.8...v0.7.0-next.9) (2026-03-30)

### Bug Fixes

- **updater:** use quitAndInstall on macOS instead of app.relaunch ([7e441cc](https://github.com/itsoltech/canopy-desktop/commit/7e441cc5708ca2689d10eebebf95ae770321c0b4))

## [0.7.0-next.8](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.7...v0.7.0-next.8) (2026-03-30)

### Features

- **sidebar:** section visibility and ordering settings ([#28](https://github.com/itsoltech/canopy-desktop/issues/28)) ([e69c684](https://github.com/itsoltech/canopy-desktop/commit/e69c684d6f30aff627b8af630ff5c227b32edab2))

## [0.7.0-next.7](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.6...v0.7.0-next.7) (2026-03-30)

### Bug Fixes

- **updater:** bypass unreliable quitAndInstall on macOS ([#27](https://github.com/itsoltech/canopy-desktop/issues/27)) ([ff2ce6f](https://github.com/itsoltech/canopy-desktop/commit/ff2ce6f0cf12eef6ac2c48617ad2ac55343d7ab6))

## [0.7.0-next.6](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.5...v0.7.0-next.6) (2026-03-30)

### Bug Fixes

- **terminal:** preserve scroll position when new content arrives ([#26](https://github.com/itsoltech/canopy-desktop/issues/26)) ([7507315](https://github.com/itsoltech/canopy-desktop/commit/75073154541f69fbea0284497add5c72a273a8ca))

## [0.7.0-next.5](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.4...v0.7.0-next.5) (2026-03-30)

### Features

- **sidebar:** agent status indicators on worktree items ([#25](https://github.com/itsoltech/canopy-desktop/issues/25)) ([52f8238](https://github.com/itsoltech/canopy-desktop/commit/52f82386c86e496c9de1e6284fb9ee0758d59216))

### Bug Fixes

- **notch:** remove 5-item cap and fix height calculation in agent list ([#24](https://github.com/itsoltech/canopy-desktop/issues/24)) ([224595f](https://github.com/itsoltech/canopy-desktop/commit/224595f48454ae856b081241fe3d4b25289040e7))

## [0.7.0-next.4](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.3...v0.7.0-next.4) (2026-03-30)

### Bug Fixes

- **ci:** add --auto flag to auto-merge gh pr merge command ([2028541](https://github.com/itsoltech/canopy-desktop/commit/20285418dd83c411141553bbe179fea8420364d9))

## [0.7.0-next.3](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.2...v0.7.0-next.3) (2026-03-30)

### Features

- post-update changelog modal with release notes ([#22](https://github.com/itsoltech/canopy-desktop/issues/22)) ([2575ef6](https://github.com/itsoltech/canopy-desktop/commit/2575ef69d2cc6e21d165163efb94529426eb5e11))

## [0.7.0-next.2](https://github.com/itsoltech/canopy-desktop/compare/v0.7.0-next.1...v0.7.0-next.2) (2026-03-29)

### Refactoring

- **sidebar:** remove file tree section from sidebar ([c24b1da](https://github.com/itsoltech/canopy-desktop/commit/c24b1da031c4b877893b88d3d62ccfd581fa800f))

## [0.7.0-next.1](https://github.com/itsoltech/canopy-desktop/compare/v0.6.2...v0.7.0-next.1) (2026-03-29)

### Features

- add file browser with read-only editor pane ([#18](https://github.com/itsoltech/canopy-desktop/issues/18)) ([9b5fb27](https://github.com/itsoltech/canopy-desktop/commit/9b5fb275b42c74f120bf8bb8f021676cb40a1440))
- next branch pre-release pipeline and update channel settings ([#19](https://github.com/itsoltech/canopy-desktop/issues/19)) ([528b0c5](https://github.com/itsoltech/canopy-desktop/commit/528b0c599cb0354a43500fbf377ed053e224f877))

### Bug Fixes

- **ci:** correct required label name to claude:review:approved ([c5f1758](https://github.com/itsoltech/canopy-desktop/commit/c5f1758b1d44e954c6c1db6847ee013b1f5cc5ac))
- **ci:** use PAT for semantic-release to bypass branch ruleset ([4c367e2](https://github.com/itsoltech/canopy-desktop/commit/4c367e2bca7a58ec4b11afb59833f5c36c1095fc))

## [0.6.2](https://github.com/itsoltech/canopy-desktop/compare/v0.6.1...v0.6.2) (2026-03-29)

### Bug Fixes

- **notch:** eliminate first-hover FPS drop ([#16](https://github.com/itsoltech/canopy-desktop/issues/16)) ([84d55a0](https://github.com/itsoltech/canopy-desktop/commit/84d55a06ff2ed873e12be22436810e0d7acd7526))
- **worktree:** dispose PTY sessions before removing worktree ([#17](https://github.com/itsoltech/canopy-desktop/issues/17)) ([ed2bb02](https://github.com/itsoltech/canopy-desktop/commit/ed2bb02ead0d6dbbdee6860cbe4d799cd45b13df))

## [0.6.1](https://github.com/itsoltech/canopy-desktop/compare/v0.6.0...v0.6.1) (2026-03-29)

### Bug Fixes

- **claude:** restore command hooks, keep only agent hooks removed ([6203b7f](https://github.com/itsoltech/canopy-desktop/commit/6203b7f6131121ea55ed59672675dfc1cb5acd71))
- **deps:** patch brace-expansion and picomatch vulnerabilities ([#15](https://github.com/itsoltech/canopy-desktop/issues/15)) ([ab65db2](https://github.com/itsoltech/canopy-desktop/commit/ab65db237c0db4a1b072a0e513e185a64ce8f122))
- **notch:** check active Claude tab before suppressing peek ([8ad0ec9](https://github.com/itsoltech/canopy-desktop/commit/8ad0ec9ed1b5cfad62d52eb8e717b2e5d394a081))
- **notch:** correct status mismatch and suppress peek for focused window ([5541b53](https://github.com/itsoltech/canopy-desktop/commit/5541b539fbd2764f5f53614de2c51d35019b81e6))

### Refactoring

- **notch:** remove session count from collapsed header ([fde97eb](https://github.com/itsoltech/canopy-desktop/commit/fde97ebf1e030254da5cf4e73464153c54391b26))

## [0.6.0](https://github.com/itsoltech/canopy-desktop/compare/v0.5.2...v0.6.0) (2026-03-29)

### Features

- **notch:** macOS notch overlay for Claude session status ([#13](https://github.com/itsoltech/canopy-desktop/issues/13)) ([d96e5e6](https://github.com/itsoltech/canopy-desktop/commit/d96e5e6b5fcd97c8721a0425a04f43ee7ceb0149))

### Bug Fixes

- **e2e:** use specific selector for reopen workspace checkbox ([4bf41a9](https://github.com/itsoltech/canopy-desktop/commit/4bf41a9934a5749872c0450fa891b938f605d1b9))
- **workspace:** restore all projects and active worktree on update restart ([#12](https://github.com/itsoltech/canopy-desktop/issues/12)) ([c62cde1](https://github.com/itsoltech/canopy-desktop/commit/c62cde1f924f984e9f526ea81875b33672801c65))

### Styling

- **css:** add tabular-nums for stable digit widths ([ac1cf6e](https://github.com/itsoltech/canopy-desktop/commit/ac1cf6e3f9c01122339c86053ff303236016b171))

## [0.5.2](https://github.com/itsoltech/canopy-desktop/compare/v0.5.1...v0.5.2) (2026-03-28)

### Bug Fixes

- **ci:** allow claude and dependabot actors in codebase audit ([6e7012d](https://github.com/itsoltech/canopy-desktop/commit/6e7012d0111f0afd0f60449a70cff6e7281fad03))
- **ci:** use valid prompt input for claude-code-action ([0df2982](https://github.com/itsoltech/canopy-desktop/commit/0df298239efb50f0f18dab3589955f96919e7a2e))
- **security:** harden Electron IPC, shell execution, and credential storage ([#4](https://github.com/itsoltech/canopy-desktop/issues/4)) ([f430c88](https://github.com/itsoltech/canopy-desktop/commit/f430c88c475226f2d54f634426b153673aeb3918))

## [0.5.1](https://github.com/itsoltech/canopy-desktop/compare/v0.5.0...v0.5.1) (2026-03-28)

### Bug Fixes

- **ui:** move [@const](https://github.com/const) declarations to valid [#each](https://github.com/itsoltech/canopy-desktop/issues/each) block scope ([de6b8e2](https://github.com/itsoltech/canopy-desktop/commit/de6b8e20cb370cc933b1a4f43107529fcaa9dbb3))

## [0.5.0](https://github.com/itsoltech/canopy-desktop/compare/v0.4.0...v0.5.0) (2026-03-28)

### Features

- **ui:** universal panel split with drag & drop ([#9](https://github.com/itsoltech/canopy-desktop/issues/9)) ([76a1729](https://github.com/itsoltech/canopy-desktop/commit/76a1729838269ebba89c56d260c64f75e6bdab16))

### Bug Fixes

- **layout:** persist layout when Claude session ID changes mid-session ([91aa2a5](https://github.com/itsoltech/canopy-desktop/commit/91aa2a5958374eea89aa5bd376b3005967cbe522))
- **release:** checkout github.sha instead of empty ref for dry-run ([626e329](https://github.com/itsoltech/canopy-desktop/commit/626e3292587abf05a428458a577139628b37e7d1))
- **release:** use triggering commit ref for dry-run builds ([81653b1](https://github.com/itsoltech/canopy-desktop/commit/81653b185d7b4fdcbff43945f0b81c6865f34c71))
- **terminal:** show reconnection status as tab badge instead of writing to terminal ([919cde4](https://github.com/itsoltech/canopy-desktop/commit/919cde49f75a1bc45428eecbba8693f65f9b7861))
