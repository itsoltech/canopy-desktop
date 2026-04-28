<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
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
  <PrefsSection title="Model & behavior">
    <PrefsRow
      label="Model"
      help="Leave empty for the Gemini CLI default"
      search="gemini model"
      layout="stacked"
    >
      <input
        id="gemini-model"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="geminiModel"
        aria-label="Gemini model"
        value={prefs.model ?? ''}
        oninput={onTextInput('model')}
        placeholder="Default"
        spellcheck="false"
      />
    </PrefsRow>

    <PrefsRow
      label="Approval mode"
      help="Controls what Gemini can do without asking. YOLO = full autonomy, Plan = read-only."
      search="gemini approval mode yolo plan auto edit"
    >
      <CustomSelect
        id="gemini-approval"
        value={prefs.approvalMode ?? ''}
        options={[
          { value: '', label: 'Default' },
          { value: 'default', label: 'Prompt' },
          { value: 'auto_edit', label: 'Auto edit' },
          { value: 'yolo', label: 'YOLO' },
          { value: 'plan', label: 'Plan (read-only)' },
        ]}
        onchange={(v) => set('approvalMode', v)}
        maxWidth="200px"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="API">
    <PrefsRow
      label="API key"
      help="Google AI API key. Falls back to GEMINI_API_KEY env variable."
      search="gemini google api key secret"
      layout="stacked"
    >
      <input
        id="gemini-apikey"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="geminiApiKey"
        aria-label="Gemini API key"
        value={apiKey}
        oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
        placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses GEMINI_API_KEY'}
        spellcheck="false"
        autocomplete="off"
      />
    </PrefsRow>
  </PrefsSection>

  <ProfileEnvVarsSection
    customEnv={prefs.customEnv}
    hint="Extra env vars passed to Gemini CLI sessions in this profile"
    onChange={(v) => set('customEnv', v)}
  />

  <PrefsSection title="Advanced">
    <PrefsRow
      label="Settings JSON override"
      help="Merged into per-session .gemini/settings.json (hooks always preserved)"
      search="gemini settings json override advanced"
      layout="stacked"
    >
      <textarea
        id="gemini-settings"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        rows="4"
        name="geminiSettingsJson"
        aria-label="Settings JSON override"
        value={prefs.settingsJson ?? ''}
        oninput={onTextInput('settingsJson')}
        placeholder={'{"key": "value"}'}
        spellcheck="false"
      ></textarea>
    </PrefsRow>
  </PrefsSection>
</div>
