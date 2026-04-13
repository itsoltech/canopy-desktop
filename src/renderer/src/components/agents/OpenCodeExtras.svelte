<script lang="ts">
  let { extra }: { extra: Record<string, unknown> } = $props()

  let pendingQuestion = $derived((extra.pendingQuestion as string | null) ?? null)
</script>

{#if pendingQuestion}
  <div class="section">
    <h4 class="section-label">Waiting for input</h4>
    <div class="question-box">
      <span class="question-text">{pendingQuestion}</span>
    </div>
  </div>
{/if}

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-label {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--c-text-faint);
    margin: 0;
  }

  .question-box {
    padding: 6px 10px;
    border-radius: 6px;
    border-left: 3px solid var(--c-warning-text);
    background: var(--c-border-subtle);
  }

  .question-text {
    font-size: 12px;
    color: var(--c-text-secondary);
    line-height: 1.4;
  }
</style>
