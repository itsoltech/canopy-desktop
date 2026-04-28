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
      // Bound only to <input>/<textarea> elements; their .value is a string.
      const target = e.target as HTMLInputElement | HTMLTextAreaElement
      // ProfilePrefs fields used here are all string | undefined, so the string value satisfies ProfilePrefs[K].
      set(key, target.value as ProfilePrefs[K])
    }
  }

  function onApiKeyInput(e: Event): void {
    // Bound to a password <input>; .value is a string.
    onApiKeyChange((e.target as HTMLInputElement).value)
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Model & behavior">
    <PrefsRow
      label="Model"
      help="Short name (sonnet, opus, haiku) or full model ID"
      search="claude model sonnet opus haiku"
      layout="stacked"
    >
      <input
        id="claude-model"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="claudeModel"
        aria-label="Claude model"
        value={prefs.model ?? ''}
        oninput={onTextInput('model')}
        placeholder="sonnet, opus, haiku, or model ID"
        spellcheck="false"
      />
    </PrefsRow>

    <PrefsRow
      label="Permission mode"
      help="Controls what Claude can do without asking. Plan = read-only, Auto = full autonomy."
      search="claude permission mode plan auto bypass accept edits"
    >
      <CustomSelect
        id="claude-perm"
        value={prefs.permissionMode ?? ''}
        options={[
          { value: '', label: 'Default' },
          { value: 'plan', label: 'Plan' },
          { value: 'auto', label: 'Auto' },
          { value: 'acceptEdits', label: 'Accept edits' },
          { value: 'bypassPermissions', label: 'Bypass permissions' },
        ]}
        onchange={(v) => set('permissionMode', v)}
        maxWidth="200px"
      />
    </PrefsRow>

    <PrefsRow
      label="Effort level"
      help="Higher effort means more thorough but slower responses"
      search="claude effort level low medium high max thinking"
    >
      <CustomSelect
        id="claude-effort"
        value={prefs.effortLevel ?? ''}
        options={[
          { value: '', label: 'Default' },
          { value: 'low', label: 'Low' },
          { value: 'medium', label: 'Medium' },
          { value: 'high', label: 'High' },
          { value: 'xhigh', label: 'Extra high' },
          { value: 'max', label: 'Max' },
        ]}
        onchange={(v) => set('effortLevel', v)}
        maxWidth="200px"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="API & provider">
    <PrefsRow
      label="API key"
      help="Anthropic API key. Falls back to ANTHROPIC_API_KEY env variable."
      search="claude anthropic api key secret"
      layout="stacked"
    >
      <input
        id="claude-apikey"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="claudeApiKey"
        aria-label="Claude API key"
        value={apiKey}
        oninput={onApiKeyInput}
        placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'sk-ant-…'}
        spellcheck="false"
        autocomplete="off"
      />
    </PrefsRow>

    <PrefsRow
      label="Base URL"
      help="Custom API endpoint. Use for Ollama, GLM, MinMax, or any OpenAI-compatible Anthropic proxy."
      search="claude base url ollama glm proxy endpoint"
      layout="stacked"
    >
      <input
        id="claude-baseurl"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="text"
        name="claudeBaseUrl"
        aria-label="Claude base URL"
        value={prefs.baseUrl ?? ''}
        oninput={onTextInput('baseUrl')}
        placeholder="https://api.anthropic.com"
        spellcheck="false"
      />
    </PrefsRow>

    <PrefsRow
      label="Provider"
      help="Cloud provider for the Claude API backend"
      search="claude provider bedrock vertex foundry aws google azure"
    >
      <CustomSelect
        id="claude-provider"
        value={prefs.provider ?? ''}
        options={[
          { value: '', label: 'Default (Anthropic)' },
          { value: 'bedrock', label: 'AWS Bedrock' },
          { value: 'vertex', label: 'Google Vertex AI' },
          { value: 'foundry', label: 'Microsoft Foundry' },
        ]}
        onchange={(v) => set('provider', v)}
        maxWidth="220px"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="System prompt">
    <PrefsRow
      label="Append to system prompt"
      help="Extra instructions added after the default system prompt in every session"
      search="claude system prompt append instructions"
      layout="stacked"
    >
      <textarea
        id="claude-sysprompt"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        rows="3"
        name="claudeSystemPrompt"
        aria-label="Append to system prompt"
        value={prefs.appendSystemPrompt ?? ''}
        oninput={onTextInput('appendSystemPrompt')}
        placeholder="Additional instructions appended to the default system prompt"
        spellcheck="false"
      ></textarea>
    </PrefsRow>
  </PrefsSection>

  <ProfileEnvVarsSection
    customEnv={prefs.customEnv}
    hint="Extra env vars passed to Claude Code sessions in this profile"
    onChange={(v) => set('customEnv', v)}
  />

  <PrefsSection title="Advanced">
    <PrefsRow
      label="Settings JSON override"
      help="Merged into per-session settings.json. Hooks and status line are always preserved."
      search="claude settings json override advanced"
      layout="stacked"
    >
      <textarea
        id="claude-settings"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        rows="4"
        name="claudeSettingsJson"
        aria-label="Settings JSON override"
        value={prefs.settingsJson ?? ''}
        oninput={onTextInput('settingsJson')}
        placeholder={'{"language": "japanese", "effortLevel": "high"}'}
        spellcheck="false"
      ></textarea>
    </PrefsRow>
  </PrefsSection>
</div>
