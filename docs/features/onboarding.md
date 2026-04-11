# Onboarding

> Guide new users through initial setup and inform returning users about new features after updates.

**Status:** Stable
**Introduced:** v0.9.0
**Platforms:** All

## Overview

Onboarding has two modes: a first-launch wizard for new installations and a feature onboarding flow shown after app updates. Both present a sequence of steps rendered as a modal overlay. Each step has a unique `id`, a `title`, a `description`, a `category` (`first-launch` or `feature`), and an `introducedIn` version string.

Step completion is persisted in the database so steps are never shown twice. On upgrade, only feature steps whose `introducedIn` version is greater than the user's previous version (and that haven't been individually completed) are shown.

## Behavior

### First-launch wizard

Shown when no onboarding record exists in the database (fresh install). The wizard has 7 ordered steps:

1. **Welcome** (id: `welcome`) -- Introduction to Canopy as a developer terminal workstation.
2. **Tool selection** (id: `tool-selection`) -- Select which AI assistants (Claude, etc.) the user plans to use. The default selection is `claude`.
3. **Environment check** (id: `environment-check`) -- Verify that selected tools are installed and accessible on the system.
4. **Theme** (id: `theme`) -- Pick a terminal color scheme.
5. **AI assistant** (id: `ai-setup`) -- Configure Claude Code to work from within Canopy.
6. **Feature customization** (id: `features`) -- Toggle features: reopen last workspace, notch overlay, WPM indicator, and others.
7. **Ready** (id: `ready`) -- Confirmation screen. Tells the user to open a project folder to get started.

Navigation: the user can move forward (Next) and backward (Back) through steps. Each forward step marks the current step as completed in the local state. Clicking Finish (or reaching the end) calls `finishOnboarding()`, which persists all step IDs and the current app version to the database via `onboarding:complete`.

Skipping: the user can skip the entire wizard at any point. This calls `skipOnboarding()`, which internally delegates to `finishOnboarding()`, marking all steps as completed so they never reappear.

### Feature onboarding (upgrade flow)

Shown when the app detects a version change and `fromVersion` is provided. The system:

1. Fetches the list of already-completed step IDs from the database via `onboarding:getCompleted`.
2. Filters all feature-category steps to those where `introducedIn` is strictly greater than `fromVersion` (semver comparison) and whose `id` has not been completed.
3. If no steps remain after filtering, onboarding mode stays `none` and nothing is shown.
4. Otherwise, the matching steps are presented in sequence the same way as the wizard.
5. On finish, all shown step IDs are marked complete in the database.

Current feature onboarding steps (as of v0.11.0):

| Step ID                    | Title                                   | Introduced in |
| -------------------------- | --------------------------------------- | ------------- |
| `task-tracker`             | Connect your task tracker               | v0.9.0        |
| `telemetry`                | Minimal telemetry                       | v0.10.0       |
| `run-configurations`       | Run Configurations                      | v0.11.0       |
| `worktree-existing-branch` | Create worktrees from existing branches | v0.10.0       |
| `remote-control`           | Remote control (Beta)                   | v0.10.0       |
| `opencode`                 | OpenCode integration                    | v0.11.0       |
| `perf-hud`                 | CPU and RAM in the status bar           | v0.11.0       |
| `pane-drag`                | Drag panes to rearrange splits          | v0.11.0       |

### State management

The `onboardingState` reactive object tracks:

- `mode`: `"none"`, `"first-launch"`, or `"upgrade"`
- `currentStep`: zero-based index into the `steps` array
- `steps`: the filtered list of `OnboardingStep` objects for the active mode
- `completedIds`: a `SvelteSet<string>` of step IDs already marked done
- `selectedTools`: a `SvelteSet<string>` for tool selection (defaults to `["claude"]`)
- `fromVersion`: the previous app version (upgrade mode only)

`initOnboarding(mode, fromVersion?)` initializes the state. `nextStep()` advances the index and marks the current step. `prevStep()` moves backward without changing completion. `currentStepDef()` returns the active step definition.

### Adding a new onboarding step

To add a feature onboarding step for a new release:

1. Add an entry to the `onboardingSteps` array in `src/renderer/src/lib/onboarding/steps.ts`.
2. Set `category: 'feature'` and `introducedIn` to the version that ships the feature.
3. No `order` field is needed for feature steps (order only applies to first-launch wizard steps).
4. The step will automatically appear for users upgrading from a version older than `introducedIn`.

## Configuration

Onboarding completion is stored in the database, not in preferences. The IPC surface:

| IPC channel               | Direction        | Payload                                                |
| ------------------------- | ---------------- | ------------------------------------------------------ |
| `onboarding:getCompleted` | renderer to main | none; returns `string[]` of completed step IDs         |
| `onboarding:complete`     | renderer to main | `{ stepIds: string[], appVersion: string }`            |
| `onboarding:reset`        | renderer to main | none; clears all completion records (used for testing) |

## Source files

- Step definitions: `src/renderer/src/lib/onboarding/steps.ts`
- Store: `src/renderer/src/lib/stores/onboarding.svelte.ts`
- Preload: `src/preload/index.ts` (onboarding section)
- Wizard step components: `src/renderer/src/components/onboarding/steps/`
