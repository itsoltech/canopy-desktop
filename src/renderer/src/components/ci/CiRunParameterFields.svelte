<script lang="ts">
  import type { CiParameter, CiRepoConfigInfo } from '../../lib/ci/types'
  import {
    isCheckboxChecked,
    multiValues,
    toggleCheckbox,
    toggleMultiValue,
  } from '../../lib/ci/runBuildForm'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  let {
    provider,
    parameters,
    values = $bindable(),
    validationErrorId,
  }: {
    provider: CiRepoConfigInfo['provider']
    parameters: CiParameter[]
    values: Record<string, string>
    validationErrorId?: string
  } = $props()

  function setAll(parameter: CiParameter, on: boolean): void {
    values[parameter.name] = on ? (parameter.options ?? []).join(parameter.valueSeparator) : ''
  }

  function isMissing(parameter: CiParameter): boolean {
    return (
      parameter.required && !parameter.hasDefault && (values[parameter.name] ?? '').trim() === ''
    )
  }

  function describedBy(parameter: CiParameter): string | undefined {
    return (
      [
        parameter.description ? `ci-run-parameter-${parameter.name}-description` : '',
        isMissing(parameter) ? validationErrorId : '',
      ]
        .filter(Boolean)
        .join(' ') || undefined
    )
  }
</script>

{#if provider === 'teamcity'}
  <p class="m-0 text-xs text-text-muted leading-snug">
    This build configuration prompts for parameters. Values below are the configuration's current
    defaults — except password parameters, which always start empty. Leave one blank to use the
    value stored on the server.
  </p>
{/if}

<div class="flex flex-col gap-3">
  {#each parameters as parameter (parameter.name)}
    <div class="flex flex-col gap-1">
      {#if parameter.kind === 'checkbox'}
        <label
          class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none"
        >
          <CustomCheckbox
            checked={isCheckboxChecked(parameter, values[parameter.name] ?? '')}
            ariaRequired={parameter.required}
            ariaInvalid={isMissing(parameter)}
            ariaDescribedby={describedBy(parameter)}
            onchange={() =>
              (values[parameter.name] = toggleCheckbox(parameter, values[parameter.name] ?? ''))}
          />
          <span>
            {parameter.label}{#if parameter.required}<span aria-hidden="true"> *</span><span
                class="sr-only"
              >
                (required)</span
              >{/if}
          </span>
        </label>
      {:else if parameter.kind === 'select' && parameter.multiple}
        <div class="flex items-center gap-2">
          <span
            id={`ci-run-parameter-${parameter.name}-label`}
            class="text-xs font-semibold text-text-faint"
          >
            {parameter.label}{#if parameter.required}<span aria-hidden="true"> *</span><span
                class="sr-only"
              >
                (required)</span
              >{/if}
          </span>
          <button
            type="button"
            class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
            onclick={() => setAll(parameter, true)}>All</button
          >
          <button
            type="button"
            class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
            onclick={() => setAll(parameter, false)}>None</button
          >
        </div>
        <div
          class="flex flex-col gap-1 pl-1"
          role="group"
          aria-labelledby={`ci-run-parameter-${parameter.name}-label`}
          aria-describedby={describedBy(parameter)}
        >
          {#each parameter.options ?? [] as option (option)}
            <label
              class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer select-none"
            >
              <CustomCheckbox
                checked={multiValues(parameter, values[parameter.name] ?? '').includes(option)}
                onchange={() =>
                  (values[parameter.name] = toggleMultiValue(
                    parameter,
                    values[parameter.name] ?? '',
                    option,
                  ))}
              />
              <span class="truncate">{option}</span>
            </label>
          {/each}
        </div>
      {:else if parameter.kind === 'select'}
        <label
          for={`ci-run-parameter-${parameter.name}`}
          class="text-xs font-semibold text-text-faint"
        >
          {parameter.label}{#if parameter.required}<span aria-hidden="true"> *</span><span
              class="sr-only"
            >
              (required)</span
            >{/if}
        </label>
        <CustomSelect
          id={`ci-run-parameter-${parameter.name}`}
          value={values[parameter.name] ?? ''}
          options={[
            ...(provider === 'github-actions' && !parameter.required
              ? [{ value: '', label: 'Use workflow default' }]
              : []),
            ...(parameter.options ?? []).map((option) => ({ value: option, label: option })),
          ]}
          onchange={(value) => (values[parameter.name] = value)}
          ariaDescribedby={describedBy(parameter)}
        />
      {:else}
        <label
          for={`ci-run-parameter-${parameter.name}`}
          class="text-xs font-semibold text-text-faint"
        >
          {parameter.label}{#if parameter.required}<span aria-hidden="true"> *</span><span
              class="sr-only"
            >
              (required)</span
            >{/if}
        </label>
        <input
          id={`ci-run-parameter-${parameter.name}`}
          class="px-2.5 py-1.5 rounded-md border border-border bg-bg-input text-sm text-text outline-none focus:border-focus-ring"
          type={parameter.kind === 'password' ? 'password' : 'text'}
          bind:value={values[parameter.name]}
          autocomplete={parameter.kind === 'password' ? 'off' : undefined}
          spellcheck="false"
          aria-describedby={describedBy(parameter)}
          aria-required={parameter.required}
          aria-invalid={isMissing(parameter)}
        />
      {/if}
      {#if parameter.description}
        <span
          id={`ci-run-parameter-${parameter.name}-description`}
          class="text-xs text-text-faint leading-snug"
        >
          {parameter.description}
        </span>
      {/if}
    </div>
  {/each}
</div>
