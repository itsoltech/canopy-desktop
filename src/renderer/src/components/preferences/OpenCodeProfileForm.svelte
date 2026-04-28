<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import ProfileEnvVarsSection from './ProfileEnvVarsSection.svelte'

  let {
    prefs,
    apiKey,
    hasApiKey,
    onPrefsChange,
    onApiKeyChange,
  }: {
    prefs: ProfilePrefs
    apiKey: string
    hasApiKey: boolean
    onPrefsChange: (next: ProfilePrefs) => void
    onApiKeyChange: (next: string) => void
  } = $props()

  function set<K extends keyof ProfilePrefs>(key: K, value: ProfilePrefs[K]): void {
    onPrefsChange({ ...prefs, [key]: value })
  }

  function onTextInput<K extends keyof ProfilePrefs>(key: K) {
    return (e: Event): void => {
      const target = e.target as HTMLInputElement | HTMLTextAreaElement
      set(key, target.value as ProfilePrefs[K])
    }
  }
</script>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    Model & behavior
  </h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="opencode-model">Model</label>
    <input
      id="opencode-model"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="provider/model"
      spellcheck="false"
    />
    <span class="text-xs text-text-faint"
      >Format: provider/model (e.g. anthropic/claude-sonnet-4-20250514)</span
    >
  </div>
</div>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">API</h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="opencode-apikey">API key</label>
    <span class="text-xs text-text-faint"
      >Set as ANTHROPIC_API_KEY. For other providers, use environment variables below</span
    >
    <input
      id="opencode-apikey"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses existing env variable'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to OpenCode sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">Advanced</h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="opencode-settings"
      >Config JSON override</label
    >
    <textarea
      id="opencode-settings"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-[60px]"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"key": "value"}'}
      spellcheck="false"
    ></textarea>
    <span class="text-xs text-text-faint">Passed via OPENCODE_CONFIG_CONTENT at session start</span>
  </div>
</div>
