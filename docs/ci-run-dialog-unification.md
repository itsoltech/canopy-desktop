# CI run dialogs — unification

**Status:** implemented · **Branch:** `feat/teamcity-ci-status` · **Last commit before this work:** `b59d4b4`

The shared implementation lives in
`src/renderer/src/components/ci/CiRunDialog.svelte`, backed by
`src/renderer/src/lib/ci/runDialogState.svelte.ts`. The router still owns configuration loading;
the dialog controller owns the provider-neutral `select → configure → confirm` state machine.

## Why

Triggering a CI run is one behaviour, but it is implemented three times:

| component                                                     | lines | role                                                 |
| ------------------------------------------------------------- | ----- | ---------------------------------------------------- |
| `src/renderer/src/components/ci/CiRunJobModal.svelte`         | 414   | TeamCity — pick build configuration + branch         |
| `src/renderer/src/components/ci/RunBuildDialog.svelte`        | 285   | TeamCity — parameters form, a **separate** component |
| `src/renderer/src/components/ci/GitHubActionsRunModal.svelte` | 504   | GitHub — every stage in **one** component            |

The third file does not come from a third provider. It comes from TeamCity's flow being split
across two components while GitHub's is not — a pre-existing asymmetry.

The cost is not theoretical. Every review note in the session that produced `b59d4b4` had to be
applied two or three times, and each repetition is where the defects came from:

- the confirmation was added to the parameters screen only, so the TeamCity path **with no
  parameters** queued on a single click — the one flow with no barrier;
- button labels, the picker swap, and the blocked-run hint each had to be changed in three files
  and drifted between them;
- a duplicate `CustomSelect` import broke compilation silently, because the same edit was being
  repeated by hand.

## Target

One `CiRunDialog.svelte` with stages `select → configure → confirm`. Provider differences become
data, not separate components:

| concern    | TeamCity                                   | GitHub                                                  |
| ---------- | ------------------------------------------ | ------------------------------------------------------- |
| job list   | build types from `ci:config`               | workflows from `ci:config`                              |
| refs       | `ciBranches(repoRoot, buildTypeId)`        | `ciJobRefs(repoRoot, jobId)` — per ref, so branch first |
| parameters | `ciBuildParameters(repoRoot, buildTypeId)` | `ciJobParameters(repoRoot, jobId, ref)`                 |
| trigger    | `ciTrigger(...)` via `triggerCiBuild`      | `ciTriggerJob(...)` via `triggerCiJob`                  |
| noun       | `build` / `parameters`                     | `workflow` / `inputs`                                   |

Everything else is already shared or trivially shareable: `BranchPicker`, `CustomSelect`,
`CiRunConfirmation`, `changedProperties`, the stage machine, the labels, the blocked-run hint.

## Behaviour that must survive the refactor

Verified live on both providers before `b59d4b4`; treat as the acceptance list.

1. **Confirmation on every path.** `Confirm` → `Start build` / `Start workflow`. Never queue on one
   click, with or without parameters.
2. **The confirmation is a SCREEN**, replacing the dialog body — not a banner appended to it.
3. **Parameters screen when there are any**, for both providers; straight to confirm when there
   are none.
4. **Back steps back** from configure/confirm, while the first screen says **Cancel** and closes.
   The header **X always closes** the whole dialog, regardless of the current stage.
5. **Branch preselects** to the worktree's branch. Nothing dispatches without the confirmation.
6. **No native OS controls**: `BranchPicker` for refs, `CustomSelect` for the job list.
7. **Blocked-run hint above the fields**, rendered only when it says something (no reserved gap).
8. **Selection changes reset** the stage and any pending confirmation.
9. **No interpretation of parameter names.** Only "changed from defaults", verbatim, passwords
   masked, unknown parameter counted as changed.
10. **Both dialogs look like siblings** — provider mark plus the server/repository line in the
    header.

## Verification

There are no component tests in this repo, so the pure logic carries the tests
(`changedProperties` in `src/renderer/src/lib/ci/runBuildForm.test.ts`) and the rest must be
exercised live, on **both** providers — see the memory note `ci-run-run-job-expected-behaviour`.

Run through, for each provider: with parameters and without; cancel from the configure screen;
change the job after reaching confirm; trigger and watch the sidebar card move to Queued then
Running.

Two combinations were never exercised live and stay open:

- **GitHub with inputs** — the repo's CI config lists workflows without inputs by default;
  `.github/workflows/release.yml` has `dry_run`. **Do not actually run `release.yml`.**
- **TeamCity without prompt parameters** — no such configuration identified on `tc.itsol.tech`.

## Method notes

- CDP cannot see native OS dialogs, and it cannot see Electron's own message boxes. Anything that
  ends in one needs a human click.
- Before any measurement, print the `aside li` indices. Worktrees move, and two false conclusions
  in this session came from clicking a stale index.
- Main-process changes need a dev-server restart; HMR only reloads the renderer. Delete `out/`
  first — electron-vite has been observed not rebuilding main otherwise.
- Do not run `npm run format` repo-wide (CRLF); format only the files you touched.
