<script lang="ts">
  let {
    targetLabel,
    target,
    refLabel,
    ref,
  }: {
    targetLabel: 'Build' | 'Workflow'
    target: string
    refLabel: 'Branch' | 'Tag'
    ref: string
  } = $props()
</script>

<dl class="m-0 flex flex-col gap-2 rounded-lg border border-border-subtle bg-bg-input px-3 py-2.5">
  <div class="flex flex-col gap-0.5 min-w-0">
    <dt class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
      {targetLabel}
    </dt>
    <dd class="m-0 text-sm font-semibold text-text leading-snug break-words">{target}</dd>
  </div>
  <div class="flex flex-col gap-0.5 min-w-0">
    <dt class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
      {refLabel}
    </dt>
    <dd class="m-0 font-mono text-sm text-text leading-snug break-all">{ref}</dd>
  </div>
</dl>
