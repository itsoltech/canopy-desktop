<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
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

  let fullAuto = $derived(prefs.fullAuto === 'true')

  function toggleFullAuto(): void {
    set('fullAuto', fullAuto ? '' : 'true')
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Model & behavior">
    <PrefsRow
      label="Model"
      help="Leave empty for Codex default"
      search="codex model openai"
      layout="stacked"
    >
      <input
        id="codex-model"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="codexModel"
        aria-label="Codex model"
        value={prefs.model ?? ''}
        oninput={onTextInput('model')}
        placeholder="Default"
        spellcheck="false"
      />
    </PrefsRow>

    <PrefsRow
      label="Approval mode"
      help="Controls when Codex pauses for human approval"
      search="codex approval mode untrusted on-request never"
    >
      <CustomSelect
        id="codex-approval"
        value={prefs.approvalMode ?? ''}
        options={[
          { value: '', label: 'Default' },
          { value: 'untrusted', label: 'Untrusted' },
          { value: 'on-request', label: 'On request' },
          { value: 'never', label: 'Never' },
        ]}
        onchange={(v) => set('approvalMode', v)}
        maxWidth="200px"
      />
    </PrefsRow>

    <PrefsRow
      label="Sandbox"
      help="Command execution policy"
      search="codex sandbox read-only workspace-write full-access"
    >
      <CustomSelect
        id="codex-sandbox"
        value={prefs.sandbox ?? ''}
        options={[
          { value: '', label: 'Default' },
          { value: 'read-only', label: 'Read only' },
          { value: 'workspace-write', label: 'Workspace write' },
          { value: 'danger-full-access', label: 'Full access' },
        ]}
        onchange={(v) => set('sandbox', v)}
        maxWidth="200px"
      />
    </PrefsRow>

    <PrefsRow
      label="Full auto"
      help="Workspace-write sandbox + on-request approvals"
      search="codex full auto autonomous"
    >
      <CustomCheckbox checked={fullAuto} onchange={toggleFullAuto} />
    </PrefsRow>

    <PrefsRow
      label="Profile"
      help="Configuration profile from config.toml"
      search="codex profile config toml"
      layout="stacked"
    >
      <input
        id="codex-profile"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="codexProfile"
        aria-label="Codex profile"
        value={prefs.profile ?? ''}
        oninput={onTextInput('profile')}
        placeholder="Default"
        spellcheck="false"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="API">
    <PrefsRow
      label="API key"
      help="OpenAI API key. Falls back to OPENAI_API_KEY env variable."
      search="codex openai api key secret"
      layout="stacked"
    >
      <input
        id="codex-apikey"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="codexApiKey"
        aria-label="Codex API key"
        value={apiKey}
        oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
        placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses OPENAI_API_KEY'}
        spellcheck="false"
        autocomplete="off"
      />
    </PrefsRow>

    <PrefsRow
      label="Base URL"
      help="Custom API endpoint (Ollama, vLLM, OpenAI-compatible proxy)"
      search="codex base url ollama vllm proxy endpoint"
      layout="stacked"
    >
      <input
        id="codex-baseurl"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="codexBaseUrl"
        aria-label="Codex base URL"
        value={prefs.baseUrl ?? ''}
        oninput={onTextInput('baseUrl')}
        placeholder="https://api.openai.com"
        spellcheck="false"
      />
    </PrefsRow>
  </PrefsSection>

  <ProfileEnvVarsSection
    customEnv={prefs.customEnv}
    hint="Extra env vars passed to Codex sessions in this profile"
    onChange={(v) => set('customEnv', v)}
  />

  <PrefsSection title="Advanced">
    <PrefsRow
      label="Settings JSON override"
      help="Merged into per-session .codex/hooks.json"
      search="codex settings json override advanced"
      layout="stacked"
    >
      <textarea
        id="codex-settings"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        rows="4"
        name="codexSettingsJson"
        aria-label="Settings JSON override"
        value={prefs.settingsJson ?? ''}
        oninput={onTextInput('settingsJson')}
        placeholder={'{"key": "value"}'}
        spellcheck="false"
      ></textarea>
    </PrefsRow>
  </PrefsSection>
</div>
