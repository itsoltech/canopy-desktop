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

  let fullAuto = $derived(prefs.fullAuto === 'true')
</script>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    Model & behavior
  </h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-model">Model</label>
    <input
      id="codex-model"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.model ?? ''}
      oninput={onTextInput('model')}
      placeholder="Default"
      spellcheck="false"
    />
    <span class="text-xs text-text-faint">Leave empty for Codex default</span>
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-approval">Approval mode</label
    >
    <span class="text-xs text-text-faint">Controls when Codex pauses for human approval</span>
    <CustomSelect
      id="codex-approval"
      value={prefs.approvalMode ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'untrusted', label: 'Untrusted' },
        { value: 'on-request', label: 'On Request' },
        { value: 'never', label: 'Never' },
      ]}
      onchange={(v) => set('approvalMode', v)}
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-sandbox">Sandbox</label>
    <span class="text-xs text-text-faint">Command execution policy</span>
    <CustomSelect
      id="codex-sandbox"
      value={prefs.sandbox ?? ''}
      options={[
        { value: '', label: 'Default' },
        { value: 'read-only', label: 'Read Only' },
        { value: 'workspace-write', label: 'Workspace Write' },
        { value: 'danger-full-access', label: 'Full Access' },
      ]}
      onchange={(v) => set('sandbox', v)}
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="flex items-center gap-2 text-sm font-medium text-text-secondary">
      <input
        type="checkbox"
        checked={fullAuto}
        onchange={() => set('fullAuto', fullAuto ? '' : 'true')}
      />
      Full auto
    </label>
    <span class="text-xs text-text-faint">Workspace-write sandbox + on-request approvals</span>
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-profile">Profile</label>
    <input
      id="codex-profile"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.profile ?? ''}
      oninput={onTextInput('profile')}
      placeholder="Default"
      spellcheck="false"
    />
    <span class="text-xs text-text-faint">Configuration profile from config.toml</span>
  </div>
</div>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">API</h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-apikey">API key</label>
    <span class="text-xs text-text-faint"
      >OpenAI API key. Falls back to OPENAI_API_KEY env variable</span
    >
    <input
      id="codex-apikey"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="password"
      value={apiKey}
      oninput={(e) => onApiKeyChange((e.target as HTMLInputElement).value)}
      placeholder={hasApiKey ? '•••• (saved — leave empty to keep)' : 'Uses OPENAI_API_KEY'}
      spellcheck="false"
      autocomplete="off"
    />
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-baseurl">Base URL</label>
    <input
      id="codex-baseurl"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      type="text"
      value={prefs.baseUrl ?? ''}
      oninput={onTextInput('baseUrl')}
      placeholder="https://api.openai.com"
      spellcheck="false"
    />
    <span class="text-xs text-text-faint"
      >Custom API endpoint (Ollama, vLLM, OpenAI-compatible proxy)</span
    >
  </div>
</div>

<ProfileEnvVarsSection
  customEnv={prefs.customEnv}
  hint="Extra env vars passed to Codex sessions in this profile"
  onChange={(v) => set('customEnv', v)}
/>

<div class="flex flex-col gap-2.5 mb-5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">Advanced</h4>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary" for="codex-settings"
      >Settings JSON override</label
    >
    <textarea
      id="codex-settings"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text font-mono text-sm outline-none focus:border-focus-ring resize-y min-h-[60px]"
      rows="4"
      value={prefs.settingsJson ?? ''}
      oninput={onTextInput('settingsJson')}
      placeholder={'{"key": "value"}'}
      spellcheck="false"
    ></textarea>
    <span class="text-xs text-text-faint">Merged into per-session .codex/hooks.json</span>
  </div>
</div>
