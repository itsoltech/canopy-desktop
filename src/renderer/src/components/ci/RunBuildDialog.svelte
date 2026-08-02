<script lang="ts">
  import { X, Play, LoaderCircle } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import type { CiParameter } from '../../lib/ci/types'
  import { cycleFocus } from '../../lib/a11y/focusTrap'
  import {
    initialFormValues,
    isCheckboxChecked,
    toggleCheckbox,
    multiValues,
    toggleMultiValue,
    missingRequired,
    toProperties,
  } from '../../lib/ci/runBuildForm'

  // Dynamic mirror of TeamCity's "Run custom build" dialog: one widget per prompt
  // parameter, prefilled with the configuration's current values.

  let {
    label,
    branch,
    parameters,
    running = false,
    error = '',
    onCancel,
    onRun,
  }: {
    label: string
    branch: string
    parameters: CiParameter[]
    running?: boolean
    /** Failure from the trigger call — shown in the footer live region. */
    error?: string
    onCancel: () => void
    onRun: (properties: Array<{ name: string; value: string }>) => void
  } = $props()

  let values = $state<Record<string, string>>(initialFormValues(parameters))
  let missing = $derived(missingRequired(parameters, values))

  let dialogEl = $state<HTMLElement>()
  $effect(() => {
    dialogEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onCancel()
      return
    }
    if (e.key === 'Tab' && dialogEl) cycleFocus(dialogEl, e)
  }

  function submit(): void {
    if (missing.length > 0 || running) return
    onRun(toProperties(parameters, values))
  }

  function setAll(param: CiParameter, on: boolean): void {
    values[param.name] = on ? (param.options ?? []).join(param.valueSeparator) : ''
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="fixed inset-0 z-[10010] flex justify-center items-center bg-scrim"
  onmousedown={onCancel}
  onkeydown={handleKeydown}
>
  <div
    bind:this={dialogEl}
    class="outline-none w-[480px] max-w-[92vw] max-h-[85vh] overflow-y-auto flex flex-col gap-3 bg-bg-overlay border border-border rounded-xl shadow-modal p-5"
    role="dialog"
    aria-modal="true"
    aria-label={`Run ${label}`}
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header class="flex items-start justify-between gap-3">
      <div class="flex flex-col gap-0.5 min-w-0">
        <h3 class="text-base font-semibold text-text m-0 leading-tight">Run {label}</h3>
        <p class="text-xs text-text-muted m-0 truncate" title={branch}>
          Branch: <span class="font-mono">{branch}</span>
        </p>
      </div>
      <button
        type="button"
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
        onclick={onCancel}
        aria-label="Close"
        title="Close"
      >
        <X size={16} />
      </button>
    </header>

    <p class="m-0 text-xs text-text-muted leading-snug">
      This build configuration prompts for parameters. Values below are the configuration's current
      defaults — except password parameters, which always start empty. Leave one blank to use the
      value stored on the server.
    </p>

    <div class="flex flex-col gap-3">
      {#each parameters as param (param.name)}
        <div class="flex flex-col gap-1">
          {#if param.kind === 'checkbox'}
            <label
              class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none"
            >
              <CustomCheckbox
                checked={isCheckboxChecked(param, values[param.name] ?? '')}
                onchange={() =>
                  (values[param.name] = toggleCheckbox(param, values[param.name] ?? ''))}
              />
              <span
                >{param.label}{#if param.required}<span class="text-danger-text" title="Required"
                    >*</span
                  >{/if}</span
              >
            </label>
          {:else if param.kind === 'select' && param.multiple}
            <div class="flex items-center gap-2">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >{param.label}{#if param.required}<span class="text-danger-text" title="Required"
                    >*</span
                  >{/if}</span
              >
              <button
                type="button"
                class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
                onclick={() => setAll(param, true)}>All</button
              >
              <button
                type="button"
                class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
                onclick={() => setAll(param, false)}>None</button
              >
            </div>
            <div class="flex flex-col gap-1 pl-1">
              {#each param.options ?? [] as option (option)}
                <label
                  class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none"
                >
                  <CustomCheckbox
                    checked={multiValues(param, values[param.name] ?? '').includes(option)}
                    onchange={() =>
                      (values[param.name] = toggleMultiValue(
                        param,
                        values[param.name] ?? '',
                        option,
                      ))}
                  />
                  <span class="truncate">{option}</span>
                </label>
              {/each}
            </div>
          {:else if param.kind === 'select'}
            <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
              >{param.label}{#if param.required}<span class="text-danger-text" title="Required"
                  >*</span
                >{/if}</span
            >
            <CustomSelect
              value={values[param.name] ?? ''}
              options={(param.options ?? []).map((o) => ({ value: o, label: o }))}
              onchange={(v) => (values[param.name] = v)}
            />
          {:else}
            <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
              >{param.label}{#if param.required}<span class="text-danger-text" title="Required"
                  >*</span
                >{/if}</span
            >
            {#if param.kind === 'password'}
              <!-- TeamCity's password parameters carry secrets — never render them
                   in the clear (bind:value needs a static type attribute). -->
              <input
                class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
                type="password"
                aria-label={param.label}
                bind:value={values[param.name]}
                autocomplete="off"
                spellcheck="false"
              />
            {:else}
              <input
                class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
                aria-label={param.label}
                bind:value={values[param.name]}
                spellcheck="false"
              />
            {/if}
          {/if}
          {#if param.description}
            <span class="text-xs text-text-faint leading-snug">{param.description}</span>
          {/if}
        </div>
      {/each}
    </div>

    <div class="flex items-center justify-end gap-1.5 pt-2 border-t border-border-subtle">
      <!-- Persistent region: a failed queue (and the required-parameters hint) swap
           in as mutations, so they are actually announced. The hint outranks a
           stale error: it explains why the button is DISABLED right now, while the
           error describes a run that already happened. -->
      <div class="mr-auto min-h-4 text-xs" aria-live="polite">
        {#if missing.length > 0}
          <span class="text-warning-text">Fill the required parameters</span>
        {:else if error}
          <span class="text-danger-text">{error}</span>
        {/if}
      </div>
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
        onclick={onCancel}>Cancel</button
      >
      <button
        type="button"
        class="flex items-center gap-1.5 px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={submit}
        disabled={missing.length > 0 || running}
      >
        {#if running}
          <LoaderCircle size={13} class="animate-spin-slow motion-reduce:animate-none" />
        {:else}
          <Play size={13} />
        {/if}
        Run Build
      </button>
    </div>
  </div>
</div>
