<script lang="ts">
  import { onDestroy } from 'svelte'
  import { captureFocusReturn } from '../../lib/a11y/focusTrap'
  import { dialogState } from '../../lib/stores/dialogs.svelte'

  // This component stays mounted while the single dialog slot moves between CI surfaces.
  // Capture before a router focuses its loading state, then restore only when the entire
  // CI dialog session closes. A Run -> Update token handoff therefore keeps the original
  // sidebar/context-menu opener instead of treating the transient button as the new opener.
  const restoreFocus = captureFocusReturn()

  onDestroy(() => {
    // A transition to another, non-CI dialog must not pull focus behind its modal scrim.
    if (dialogState.current.type === 'none') restoreFocus()
  })
</script>
