<script lang="ts">
  import type { ProfilePrefs } from '../../../../main/profiles/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
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
    <label class="text-sm font-medium text-text-secondary" for="claude-model">Model</label>
    <span class="text-xs text-text-faint">Short name (sonnet, opus, haiku) or full model ID</span>
    <input
      id="claude-model"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="sonnet, opus, haiku, or model ID"
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-perm">Permission mode</label>
    <span class="text-xs text-text-faint"
      >Controls what Claude can do without asking. Plan = read-only, Auto = full autonomy</span
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
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-effort">Effort level</label>
    <span class="text-xs text-text-faint"
      >Higher effort means more thorough but slower responses</span
    >
    <CustomSelect
      id="claude-effort"
      value={prefs.effortLevel ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'low', label: 'Low' },
        { value: 'medium', label: 'Medium' },
        { value: 'high', label: 'High' },
        { value: 'xhigh', label: 'Extra High' },
        { value: 'max', label: 'Max' },
      ]}
      onchange={(v) => set('effortLevel', v)}
    />
  </div>
</div>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    API / Provider
  </h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-apikey">API key</label>
    <span class="text-xs text-text-faint"
      >Anthropic API key. Falls back to ANTHROPIC_API_KEY env variable</span
    >
    <input
      id="claude-apikey"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'sk-ant-...'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-baseurl">Base URL</label>
    <span class="text-xs text-text-faint"
      >Custom API endpoint. Use for Ollama, GLM, MinMax, or any OpenAI-compatible Anthropic proxy.</span
    >
    <input
      id="claude-baseurl"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.baseUrl ?? ''}
      oninput={onTextInput('baseUrl')}
      placeholder="https://api.anthropic.com"
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-provider">Provider</label>
    <span class="text-xs text-text-faint">Cloud provider for the Claude API backend</span>
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
    />
  </div>
</div>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    System prompt
  </h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-sysprompt"
      >Append to system prompt</label
    >
    <span class="text-xs text-text-faint"
      >Extra instructions added after the default system prompt in every session</span
    >
    <textarea
      id="claude-sysprompt"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring resize-y min-h-[60px]"
      rows="3"
      value={prefs.appendSystemPrompt ?? ''}
      oninput={onTextInput('appendSystemPrompt')}
      placeholder="Additional instructions appended to the default system prompt"
      spellcheck="false"
    ></textarea>
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Claude Code sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">Advanced</h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="claude-settings"
      >Settings JSON override</label
    >
    <textarea
      id="claude-settings"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-[60px]"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"language": "japanese", "effortLevel": "high"}'}
      spellcheck="false"
    ></textarea>
    <span class="text-xs text-text-faint"
      >Merged into per-session settings.json. Hooks and status line are always preserved.</span
    >
  </div>
</div>
