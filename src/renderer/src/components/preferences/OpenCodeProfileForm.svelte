<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import ProfileEnvVarsSection from './ProfileEnvVarsSection.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection title="Model">
    <PrefsRow
      label="Model"
      help="Format: provider/model (e.g. anthropic/claude-sonnet-4-20250514)"
      search="opencode model provider anthropic openai"
      layout="stacked"
    >
      <input
        id="opencode-model"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="opencodeModel"
        aria-label="OpenCode model"
        value={prefs.model ?? ''}
        oninput={onTextInput('model')}
        placeholder="provider/model"
        spellcheck="false"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="API">
    <PrefsRow
      label="API key"
      help="Set as ANTHROPIC_API_KEY. For other providers, use environment variables below."
      search="opencode api key secret anthropic"
      layout="stacked"
    >
      <input
        id="opencode-apikey"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="opencodeApiKey"
        aria-label="OpenCode API key"
        value={apiKey}
        oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
        placeholder={hasApiKey
          ? '•••• (saved — leave empty to keep)'
          : 'Uses existing env variable'}
        spellcheck="false"
        autocomplete="off"
      />
    </PrefsRow>
  </PrefsSection>

  <ProfileEnvVarsSection
    customEnv={prefs.customEnv}
    hint="Extra env vars passed to OpenCode sessions in this profile"
    onChange={(v) => set('customEnv', v)}
  />

  <PrefsSection title="Advanced">
    <PrefsRow
      label="Config JSON override"
      help="Passed via OPENCODE_CONFIG_CONTENT at session start"
      search="opencode config json override advanced"
      layout="stacked"
    >
      <textarea
        id="opencode-settings"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        rows="4"
        name="opencodeSettingsJson"
        aria-label="Config JSON override"
        value={prefs.settingsJson ?? ''}
        oninput={onTextInput('settingsJson')}
        placeholder={'{"key": "value"}'}
        spellcheck="false"
      ></textarea>
    </PrefsRow>
  </PrefsSection>
</div>
