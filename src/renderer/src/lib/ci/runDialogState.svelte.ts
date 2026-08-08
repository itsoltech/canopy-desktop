import { untrack } from 'svelte'
import { match } from 'ts-pattern'
import { closeDialog } from '../stores/dialogs.svelte'
import { triggerCiBuild, triggerCiJob } from '../stores/ci.svelte'
import {
  changedProperties,
  initialFormValues,
  missingRequired,
  toInputs,
  toProperties,
} from './runBuildForm'
import type { CiParameter, CiRef, CiRepoConfigInfo } from './types'

export type CiRunStage = 'select' | 'configure' | 'confirm'

export function nextCiRunStage(stage: CiRunStage, hasParameters: boolean): CiRunStage | null {
  return match(stage)
    .with('select', () => (hasParameters ? 'configure' : 'confirm') as CiRunStage)
    .with('configure', () => 'confirm' as const)
    .with('confirm', () => null)
    .exhaustive()
}

export function previousCiRunStage(stage: CiRunStage, hasParameters: boolean): CiRunStage | null {
  return match(stage)
    .with('confirm', () => (hasParameters ? 'configure' : 'select') as CiRunStage)
    .with('configure', () => 'select' as const)
    .with('select', () => null)
    .exhaustive()
}

export function ambiguousCiRefNames(refs: CiRef[]): string[] {
  return refs
    .filter(
      (ref, index) =>
        refs.findIndex((candidate) => candidate.name === ref.name) === index &&
        refs.some((candidate) => candidate.name === ref.name && candidate.kind !== ref.kind),
    )
    .map((ref) => ref.name)
    .sort()
}

export function isGitHubDispatchDenied(
  provider: CiRepoConfigInfo['provider'],
  status: number | undefined,
): boolean {
  return provider === 'github-actions' && status === 403
}

// Keep this factory inferred so the exported ReturnType stays aligned with its reactive getters.
// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export function createCiRunDialogState(
  repoRoot: string,
  initialBranch: string | undefined,
  config: CiRepoConfigInfo,
) {
  let jobId = $state('')
  let refs = $state<CiRef[]>([])
  let selectedRefName = $state('')
  let refQuery = $state('')
  let parameters = $state<CiParameter[] | null>(null)
  let schemaRevision = $state('')
  let values = $state<Record<string, string>>({})
  let stage = $state<CiRunStage>('select')
  let refsLoading = $state(false)
  let parametersLoading = $state(false)
  let running = $state(false)
  let refsError = $state('')
  let parametersError = $state('')
  let triggerError = $state('')
  let triggerErrorStatus = $state<number | undefined>(undefined)
  let refsSequence = 0
  let parametersSequence = 0

  const isTeamCity = config.provider === 'teamcity'
  const jobs =
    config.provider === 'teamcity'
      ? config.buildTypes.map((buildType) => ({ value: buildType.id, label: buildType.label }))
      : config.workflows.map((workflow) => ({ value: workflow.path, label: workflow.label }))
  const providerName = isTeamCity ? 'TeamCity' : 'GitHub Actions'
  const runNoun = isTeamCity ? 'build' : 'workflow'
  const jobNoun = isTeamCity ? 'job' : 'workflow'
  const parameterNoun = isTeamCity ? 'parameters' : 'inputs'
  const refsNoun = isTeamCity ? 'branches' : 'branches and tags'
  const providerLine = config.provider === 'teamcity' ? config.baseUrl : config.repository
  const refLabel = isTeamCity ? 'Branch' : 'Branch or tag'

  const jobLabel = $derived(jobs.find((job) => job.value === jobId)?.label ?? jobId)
  const ambiguousRefNames = $derived(ambiguousCiRefNames(refs))
  const selectableRefs = $derived(refs.filter((ref) => !ambiguousRefNames.includes(ref.name)))
  const branchNames = $derived(
    selectableRefs.filter((ref) => ref.kind === 'branch').map((ref) => ref.name),
  )
  const tagNames = $derived(
    selectableRefs.filter((ref) => ref.kind === 'tag').map((ref) => ref.name),
  )
  const selectedRef = $derived(
    config.provider === 'teamcity'
      ? selectedRefName
        ? ({ name: selectedRefName, kind: 'branch' } satisfies CiRef)
        : undefined
      : selectedRefName
        ? (selectableRefs.find((ref) => ref.kind === 'branch' && ref.name === selectedRefName) ??
          selectableRefs.find((ref) => ref.name === selectedRefName))
        : undefined,
  )
  const hasParameters = $derived((parameters?.length ?? 0) > 0)
  const missing = $derived(parameters ? missingRequired(parameters, values) : [])
  const submitted = $derived.by(() => {
    if (!parameters) return []
    if (config.provider === 'teamcity') return toProperties(parameters, values)
    return parameters.map((parameter) => ({
      name: parameter.name,
      value: values[parameter.name] ?? '',
    }))
  })
  const changed = $derived(parameters ? changedProperties(parameters, submitted) : [])
  const loading = $derived(refsLoading || parametersLoading)
  const selectionReady = $derived(
    !!jobId && !!selectedRef && parameters !== null && !loading && !refsError && !parametersError,
  )
  const canContinue = $derived(
    selectionReady && (stage === 'select' || missing.length === 0) && !running,
  )
  const dispatchDenied = $derived(isGitHubDispatchDenied(config.provider, triggerErrorStatus))
  const worktreeBranchMissing = $derived(
    config.provider === 'github-actions' &&
      !!initialBranch &&
      !selectedRefName &&
      !refsLoading &&
      !refsError &&
      !selectableRefs.some((ref) => ref.kind === 'branch' && ref.name === initialBranch),
  )

  const runBlockedReason = $derived(
    running
      ? `Disabled while the ${runNoun} request is in flight`
      : refsLoading
        ? `Disabled: loading ${refsNoun}…`
        : parametersLoading
          ? `Disabled: loading ${parameterNoun}…`
          : refsError
            ? `Disabled: ${refsNoun} could not be loaded`
            : parametersError
              ? `Disabled: ${parameterNoun} could not be loaded`
              : !jobId
                ? `Disabled: pick a ${jobNoun} first`
                : !selectedRef
                  ? `Disabled: pick a ${isTeamCity ? 'branch' : 'remote branch or tag'}`
                  : stage !== 'select' && missing.length > 0
                    ? `Disabled: fill the required ${parameterNoun} first`
                    : '',
  )
  const runBlockedHint = $derived(
    stage !== 'select' || running || loading || refsError || parametersError
      ? ''
      : !jobId
        ? `Pick a ${jobNoun} before running.`
        : !selectedRef
          ? isTeamCity
            ? 'Pick a branch: open the list or type to search. Typing clears the current selection.'
            : 'Pick a remote branch or tag before running.'
          : '',
  )
  const primaryLabel = $derived(
    running
      ? isTeamCity
        ? 'Queueing…'
        : 'Starting…'
      : loading
        ? 'Loading…'
        : refsError || parametersError
          ? 'Unavailable'
          : stage === 'confirm'
            ? `Start ${runNoun}`
            : stage === 'select' && hasParameters
              ? 'Configure'
              : 'Confirm',
  )
  const primaryTitle = $derived.by(() => {
    if (runBlockedReason) return runBlockedReason
    if (stage === 'confirm') return `Start the selected ${runNoun}`
    if (stage === 'configure') return `Review this ${runNoun} before starting`
    return hasParameters
      ? `Review this ${runNoun}'s ${parameterNoun} before starting`
      : `Review the selected ${runNoun} before starting`
  })
  const visibleError = $derived(
    stage !== 'select' && missing.length > 0
      ? `Fill the required ${parameterNoun}.`
      : refsError || parametersError || triggerError,
  )

  // GitHub input schemas belong to a workflow AND ref. TeamCity prompt parameters belong only to
  // the build configuration, so that provider loads them alongside its branch list instead.
  let loadedGitHubParametersFor = ''
  $effect(() => {
    if (config.provider !== 'github-actions') return
    const selection = selectedRefName ? `${jobId}\u0000${selectedRefName}` : ''
    if (selection === loadedGitHubParametersFor) return
    loadedGitHubParametersFor = selection
    untrack(() => {
      parametersSequence += 1
      parameters = null
      schemaRevision = ''
      values = {}
      parametersError = ''
      parametersLoading = false
      if (selection) void loadParameters()
    })
  })

  function initialize(): void {
    jobId = jobs[0]?.value ?? ''
    if (!jobId) {
      parameters = []
      return
    }

    if (config.provider === 'teamcity') {
      selectedRefName = initialBranch ?? ''
      refQuery = selectedRefName
      void loadRefs()
      void loadParameters()
    } else {
      void loadRefs()
    }
  }

  function cancelOrBack(): void {
    if (running) return
    const previous = previousCiRunStage(stage, hasParameters)
    if (previous) stage = previous
    else closeDialog()
  }

  function close(): void {
    if (running) return
    closeDialog()
  }

  async function selectJob(value: string): Promise<void> {
    jobId = value
    stage = 'select'
    refsSequence += 1
    parametersSequence += 1
    refs = []
    parameters = null
    schemaRevision = ''
    values = {}
    refsError = ''
    parametersError = ''
    triggerError = ''
    triggerErrorStatus = undefined
    refsLoading = false
    parametersLoading = false
    loadedGitHubParametersFor = ''

    if (config.provider === 'teamcity') {
      selectedRefName = initialBranch ?? ''
      refQuery = selectedRefName
      void loadRefs()
      void loadParameters()
    } else {
      selectedRefName = ''
      refQuery = ''
      await loadRefs()
    }
  }

  async function loadRefs(): Promise<void> {
    if (!jobId) return
    const sequence = ++refsSequence
    const requestedJobId = jobId
    refsLoading = true
    refsError = ''
    try {
      if (config.provider === 'teamcity') {
        const branches = await window.api.ciBranches(repoRoot, requestedJobId)
        if (sequence !== refsSequence || requestedJobId !== jobId) return
        refs = branches.map((name) => ({ name, kind: 'branch' }))
        // Never replace a worktree preselect with TeamCity's first/default branch.
      } else {
        const loadedRefs = await window.api.ciJobRefs(repoRoot, requestedJobId)
        if (sequence !== refsSequence || requestedJobId !== jobId) return
        refs = loadedRefs
        const ambiguousNames = ambiguousCiRefNames(loadedRefs)
        const availableRefs = loadedRefs.filter((ref) => !ambiguousNames.includes(ref.name))
        const existing = selectedRefName
          ? availableRefs.find((ref) => ref.name === selectedRefName)
          : undefined
        const worktreeRef = initialBranch
          ? availableRefs.find((ref) => ref.kind === 'branch' && ref.name === initialBranch)
          : undefined
        selectedRefName = existing?.name ?? worktreeRef?.name ?? ''
        refQuery = selectedRefName
      }
    } catch (cause) {
      if (sequence !== refsSequence || requestedJobId !== jobId) return
      refs = []
      refsError =
        cause instanceof Error
          ? cause.message
          : isTeamCity
            ? 'Failed to load branches'
            : 'Could not load GitHub refs'
    } finally {
      if (sequence === refsSequence) refsLoading = false
    }
  }

  async function loadParameters(): Promise<void> {
    if (!jobId || (config.provider === 'github-actions' && !selectedRef)) return
    const sequence = ++parametersSequence
    const requestedJobId = jobId
    const requestedRefName = selectedRefName
    parametersLoading = true
    parametersError = ''
    parameters = null
    try {
      if (config.provider === 'teamcity') {
        const loaded = await window.api.ciBuildParameters(repoRoot, requestedJobId)
        if (sequence !== parametersSequence || requestedJobId !== jobId) return
        parameters = loaded
        values = initialFormValues(loaded)
      } else if (selectedRef) {
        const result = await window.api.ciJobParameters(
          repoRoot,
          requestedJobId,
          $state.snapshot(selectedRef),
        )
        if (
          sequence !== parametersSequence ||
          requestedJobId !== jobId ||
          requestedRefName !== selectedRefName
        )
          return
        parameters = result.parameters
        schemaRevision = result.schemaRevision
        values = initialFormValues(result.parameters)
      }
    } catch (cause) {
      if (
        sequence !== parametersSequence ||
        requestedJobId !== jobId ||
        requestedRefName !== selectedRefName
      )
        return
      parametersError =
        cause instanceof Error
          ? cause.message
          : isTeamCity
            ? 'Failed to load build parameters'
            : 'Could not load workflow inputs'
    } finally {
      if (sequence === parametersSequence) parametersLoading = false
    }
  }

  function primaryAction(): void {
    if (!canContinue) return
    const next = nextCiRunStage(stage, hasParameters)
    if (next) stage = next
    else void triggerRun()
  }

  async function triggerRun(): Promise<void> {
    if (!canContinue || !selectedRef || !parameters) return
    running = true
    triggerError = ''
    triggerErrorStatus = undefined
    try {
      if (config.provider === 'teamcity') {
        const properties = parameters.length > 0 ? toProperties(parameters, values) : undefined
        const failure = await triggerCiBuild(
          repoRoot,
          jobId,
          selectedRef.name,
          jobLabel,
          properties,
        )
        if (failure) triggerError = failure
        else closeDialog()
        return
      }

      const issue = await triggerCiJob(
        repoRoot,
        {
          jobId,
          ref: { name: selectedRef.name, kind: selectedRef.kind },
          schemaRevision,
          inputs: toInputs(parameters, values),
        },
        jobLabel,
      )
      if (issue?.kind === 'cancelled') return
      if (issue?.kind === 'failure') {
        triggerErrorStatus = issue.status
        if (issue.code === 'CiWorkflowSchemaChanged') {
          await loadParameters()
          stage = (parameters?.length ?? 0) > 0 ? 'configure' : 'select'
          triggerError = parametersError ? `${issue.message} ${parametersError}` : issue.message
        } else {
          triggerError = issue.message
        }
        return
      }
      closeDialog()
    } finally {
      running = false
    }
  }

  return {
    config,
    initialBranch,
    isTeamCity,
    jobs,
    providerName,
    runNoun,
    parameterNoun,
    providerLine,
    refLabel,
    get jobId() {
      return jobId
    },
    get selectedRefName() {
      return selectedRefName
    },
    set selectedRefName(value: string) {
      if (selectedRefName === value) return
      selectedRefName = value
      // A confirmation must never outlive the ref it describes.
      stage = 'select'
      triggerError = ''
      triggerErrorStatus = undefined
    },
    get refQuery() {
      return refQuery
    },
    set refQuery(value: string) {
      refQuery = value
    },
    get parameters() {
      return parameters
    },
    get values() {
      return values
    },
    set values(next: Record<string, string>) {
      values = next
    },
    get stage() {
      return stage
    },
    get refsLoading() {
      return refsLoading
    },
    get running() {
      return running
    },
    get parametersError() {
      return parametersError
    },
    get triggerError() {
      return triggerError
    },
    get branchNames() {
      return branchNames
    },
    get tagNames() {
      return tagNames
    },
    get ambiguousRefNames() {
      return ambiguousRefNames
    },
    get selectedRef() {
      return selectedRef
    },
    get jobLabel() {
      return jobLabel
    },
    get submitted() {
      return submitted
    },
    get changed() {
      return changed
    },
    get missing() {
      return missing
    },
    get loading() {
      return loading
    },
    get canContinue() {
      return canContinue
    },
    get dispatchDenied() {
      return dispatchDenied
    },
    get worktreeBranchMissing() {
      return worktreeBranchMissing
    },
    get runBlockedHint() {
      return runBlockedHint
    },
    get primaryLabel() {
      return primaryLabel
    },
    get primaryTitle() {
      return primaryTitle
    },
    get visibleError() {
      return visibleError
    },
    initialize,
    close,
    cancelOrBack,
    selectJob,
    loadRefs,
    loadParameters,
    primaryAction,
  }
}

export type CiRunDialogState = ReturnType<typeof createCiRunDialogState>
