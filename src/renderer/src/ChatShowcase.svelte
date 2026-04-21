<script lang="ts">
  import { Trash2, Pencil, RotateCcw, Share, MoreHorizontal } from '@lucide/svelte'

  // Atoms
  import Avatar from './components/chat/atoms/Avatar.svelte'
  import RoleBadge from './components/chat/atoms/RoleBadge.svelte'
  import ModelBadge from './components/chat/atoms/ModelBadge.svelte'
  import Timestamp from './components/chat/atoms/Timestamp.svelte'
  import TokenCount from './components/chat/atoms/TokenCount.svelte'
  import StatusDot from './components/chat/atoms/StatusDot.svelte'
  import IconButton from './components/chat/atoms/IconButton.svelte'
  import CopyButton from './components/chat/atoms/CopyButton.svelte'
  import TypingDots from './components/chat/atoms/TypingDots.svelte'
  import Kbd from './components/chat/atoms/Kbd.svelte'
  import Divider from './components/chat/atoms/Divider.svelte'
  import InlineCode from './components/chat/atoms/InlineCode.svelte'
  import Chip from './components/chat/atoms/Chip.svelte'
  import SlashToken from './components/chat/atoms/SlashToken.svelte'
  import FilePath from './components/chat/atoms/FilePath.svelte'
  import SubAgentBadge from './components/chat/atoms/SubAgentBadge.svelte'

  // Molecules
  import MessageHeader from './components/chat/molecules/MessageHeader.svelte'
  import MessageActions from './components/chat/molecules/MessageActions.svelte'
  import MessageMeta from './components/chat/molecules/MessageMeta.svelte'
  import MessageBubble from './components/chat/molecules/MessageBubble.svelte'
  import CodeBlock from './components/chat/molecules/CodeBlock.svelte'
  import ToolCallBlock from './components/chat/molecules/ToolCallBlock.svelte'
  import ThinkingBlock from './components/chat/molecules/ThinkingBlock.svelte'
  import AttachmentChip from './components/chat/molecules/AttachmentChip.svelte'
  import AttachmentTray from './components/chat/molecules/AttachmentTray.svelte'
  import SlashCommandHint from './components/chat/molecules/SlashCommandHint.svelte'
  import SendControl from './components/chat/molecules/SendControl.svelte'
  import ModelPickerInline from './components/chat/molecules/ModelPickerInline.svelte'
  import ChatInput from './components/chat/molecules/ChatInput.svelte'
  import SubAgentRun from './components/chat/molecules/SubAgentRun.svelte'
  import AttentionBanner from './components/chat/molecules/AttentionBanner.svelte'
  import QuestionOption from './components/chat/molecules/QuestionOption.svelte'
  import QuestionBlock from './components/chat/molecules/QuestionBlock.svelte'
  import QuestionnaireBlock from './components/chat/molecules/QuestionnaireBlock.svelte'
  import PlanApprovalBlock from './components/chat/molecules/PlanApprovalBlock.svelte'
  import ToolPermissionBlock from './components/chat/molecules/ToolPermissionBlock.svelte'

  const now = Date.now()

  let attachments = $state([
    { id: 1, name: 'screenshot.png', size: 204_800, kind: 'image' as const },
    { id: 2, name: 'notes.md', size: 1_240, kind: 'text' as const },
    { id: 3, name: 'report-final-v3.pdf', size: 2_400_000, kind: 'file' as const },
  ])

  let selectedModel = $state('claude-opus-4-7')
  const modelOptions = [
    { value: 'claude-opus-4-7', label: 'claude-opus-4-7' },
    { value: 'claude-sonnet-4-6', label: 'claude-sonnet-4-6' },
    { value: 'claude-haiku-4-5', label: 'claude-haiku-4-5' },
    { value: 'gpt-5', label: 'gpt-5' },
    { value: 'gemini-2.5-pro', label: 'gemini-2.5-pro' },
  ]

  let chatValue = $state('')
  let submissions = $state<string[]>([])

  function handleSubmit(text: string): void {
    submissions = [text, ...submissions].slice(0, 5)
  }

  function removeAttachment(id: number): void {
    attachments = attachments.filter((a) => a.id !== id)
  }

  const sampleCode = `export async function greet(name: string): Promise<string> {
  const response = await fetch(\`/api/hello?name=\${name}\`)
  if (!response.ok) throw new Error('request failed')
  return response.json()
}`

  const toolArgs = `{
  "pattern": "TODO",
  "path": "src/",
  "output_mode": "content"
}`

  const toolResult = `src/renderer/src/App.svelte:12:  // TODO: restore workspace
src/main/terminal.ts:48:  // TODO: handle EBADF
2 matches in 2 files`

  const thinkingContent = `The user wants a showcase. The cleanest approach is a second HTML entry that reuses the tokens.css and mounts a dedicated Svelte page. I should cover every atom with at least two prop combinations and every molecule with representative states.`

  const editOldString = `  const response = await fetch('/api/hello')
  if (!response.ok) throw new Error('failed')
  return response.json()`

  const editNewString = `  const response = await fetch(\`/api/hello?name=\${name}\`)
  if (!response.ok) throw new Error('request failed')
  const data = await response.json()
  return data`

  const writeContent = `export function slugify(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
}`

  const readResult = `  12  import { mount } from 'svelte'
  13
  14  import './assets/main.css'
  15
  16  import App from './App.svelte'
  17
  18  const app = mount(App, {
  19    target: document.getElementById('app')!,
  20  })`

  const bashResult = `total 48K
-rw-r--r--  1 nix  staff  4.1K Apr 21 10:02 tokens.css
-rw-r--r--  1 nix  staff  1.6K Apr 21 10:15 CustomCheckbox.svelte
-rw-r--r--  1 nix  staff  7.0K Apr 21 10:15 CustomSelect.svelte
drwxr-xr-x  8 nix  staff   256 Apr 21 11:02 chat
3 directories, 27 files`

  const grepResult = `src/renderer/src/App.svelte:12:  // TODO: restore workspace
src/main/terminal.ts:48:  // TODO: handle EBADF
src/main/taskTracker/index.ts:94:  // TODO: retry on 429`

  const globResult = `src/renderer/src/components/chat/atoms/Avatar.svelte
src/renderer/src/components/chat/atoms/Chip.svelte
src/renderer/src/components/chat/atoms/CopyButton.svelte
src/renderer/src/components/chat/atoms/FilePath.svelte
src/renderer/src/components/chat/atoms/IconButton.svelte
src/renderer/src/components/chat/atoms/Kbd.svelte
src/renderer/src/components/chat/atoms/ModelBadge.svelte`

  const webSearchResult = `Svelte 5 runes docs — svelte.dev/docs/svelte/what-are-runes
The Svelte 5 runes tutorial — svelte.dev/tutorial/svelte/universal-reactivity
A deep dive into $state and $derived — joyofcode.xyz/svelte-5-runes`

  const bashErrorResult = 'rm: /protected: Permission denied\nexit code: 1'

  const fileIndices = Array.from({ length: 12 }, (_v, i) => i)

  // ── User-attention demo data ──────────────────────────────────────────────

  const libraryQuestion = {
    question: 'Which library should we use for date formatting?',
    header: 'Library',
    options: [
      {
        label: 'date-fns (Recommended)',
        description: 'Tree-shakeable, modular, already used elsewhere in the codebase.',
      },
      {
        label: 'dayjs',
        description: 'Smaller bundle, Moment-compatible API.',
      },
      {
        label: 'luxon',
        description: 'Strong timezone support but larger footprint.',
      },
    ],
  }

  const featuresQuestion = {
    question: 'Which features do you want to enable?',
    header: 'Features',
    multiSelect: true,
    options: [
      { label: 'Keyboard shortcuts', description: '⌘K palette, navigation hotkeys.' },
      { label: 'Notifications', description: 'Desktop notifications for completed tasks.' },
      { label: 'Telemetry', description: 'Anonymous usage statistics.' },
      { label: 'Beta features', description: 'Experimental functionality (may be unstable).' },
    ],
  }

  const approachQuestion = {
    question: 'Which implementation approach?',
    header: 'Approach',
    options: [
      {
        label: 'Inline component (Recommended)',
        description: 'Single file, minimal API surface.',
        preview: `let count = $state(0)\n\nfunction increment() {\n  count++\n}\n\n// render <button>{count}</button>`,
      },
      {
        label: 'Extracted hook',
        description: 'Reusable across components, more boilerplate.',
        preview: `function useCounter() {\n  let count = $state(0)\n  return {\n    get value() { return count },\n    inc: () => count++,\n  }\n}`,
      },
      {
        label: 'Context store',
        description: 'Shared state across tree; requires wiring at root.',
        preview: `const ctx = setContext('counter', {\n  count: $state(0),\n})\n\n// in children:\nconst { count } = getContext('counter')`,
      },
    ],
  }

  const samplePlan = `## Summary\n\nSwap out the 3-dot bouncer for a Braille cli-spinners style animation. Rewrite the TypingDots atom to tick through 10 Braille frames at 80ms each, respecting prefers-reduced-motion.\n\n## Files\n- src/renderer/src/components/chat/atoms/TypingDots.svelte\n\n## Verification\n- npm run svelte-check\n- Manual eyeball at /showcase`

  const planPermissions = [
    { tool: 'Edit', prompt: 'modify TypingDots.svelte' },
    { tool: 'Bash', prompt: 'run svelte-check' },
    { tool: 'Bash', prompt: 'run lint' },
  ]

  let submittedAnswers = $state<Record<string, unknown> | null>(null)
  let permissionLog = $state<string[]>([])

  function logPermission(tool: string, decision: string): void {
    permissionLog = [`${tool} → ${decision}`, ...permissionLog].slice(0, 5)
  }
</script>

<div class="showcase">
  <header class="page-header">
    <h1>Chat UI Showcase</h1>
    <p class="subtitle">
      All atoms and molecules from <code>components/chat/</code> rendered against Canopy tokens. Open
      DevTools to inspect the DOM.
    </p>
  </header>

  <!-- ────────────────────────── ATOMS ────────────────────────── -->
  <section class="section">
    <h2>Atoms</h2>

    <article class="demo">
      <h3>Avatar</h3>
      <div class="row">
        <Avatar role="assistant" model="ClaudeAI" />
        <Avatar role="assistant" model="OpenAI" />
        <Avatar role="assistant" model="Gemini" />
        <Avatar role="user" initial="D" />
        <Avatar role="tool" initial="T" />
        <Avatar role="system" />
        <Avatar role="assistant" model="ClaudeAI" size={36} />
        <Avatar role="user" initial="N" size={20} />
      </div>
    </article>

    <article class="demo">
      <h3>RoleBadge</h3>
      <div class="row">
        <RoleBadge role="user" />
        <RoleBadge role="assistant" />
        <RoleBadge role="tool" />
        <RoleBadge role="system" />
        <RoleBadge role="assistant" label="Claude" />
      </div>
    </article>

    <article class="demo">
      <h3>ModelBadge</h3>
      <div class="row">
        <ModelBadge model="claude-opus-4-7" />
        <ModelBadge model="gpt-5" />
        <ModelBadge model="gemini-2.5-pro" />
      </div>
    </article>

    <article class="demo">
      <h3>Timestamp</h3>
      <div class="row">
        <Timestamp timestamp={now - 15_000} />
        <Timestamp timestamp={now - 90_000} />
        <Timestamp timestamp={now - 3_600_000} />
        <Timestamp timestamp={now - 86_400_000 * 3} />
        <span class="hint">hover for absolute time</span>
      </div>
    </article>

    <article class="demo">
      <h3>TokenCount</h3>
      <div class="row">
        <TokenCount tokens={42} />
        <TokenCount tokens={1240} />
        <TokenCount tokens={180_000} />
        <TokenCount tokens={2_300_000} label="ctx" />
      </div>
    </article>

    <article class="demo">
      <h3>StatusDot</h3>
      <div class="row">
        <span class="stack">
          <StatusDot status="idle" />
          <span class="caption">idle</span>
        </span>
        <span class="stack">
          <StatusDot status="thinking" pulse />
          <span class="caption">thinking</span>
        </span>
        <span class="stack">
          <StatusDot status="tool" pulse />
          <span class="caption">tool</span>
        </span>
        <span class="stack">
          <StatusDot status="success" />
          <span class="caption">success</span>
        </span>
        <span class="stack">
          <StatusDot status="warning" />
          <span class="caption">warning</span>
        </span>
        <span class="stack">
          <StatusDot status="error" />
          <span class="caption">error</span>
        </span>
        <span class="stack">
          <StatusDot status="inactive" />
          <span class="caption">inactive</span>
        </span>
        <span class="stack">
          <StatusDot status="thinking" size={12} pulse />
          <span class="caption">size 12</span>
        </span>
      </div>
    </article>

    <article class="demo">
      <h3>IconButton</h3>
      <div class="row">
        <IconButton tooltip="Edit" label="Edit"><Pencil size={14} /></IconButton>
        <IconButton tooltip="Regenerate" label="Regenerate">
          <RotateCcw size={14} />
        </IconButton>
        <IconButton tooltip="Share" label="Share" variant="primary">
          <Share size={14} />
        </IconButton>
        <IconButton tooltip="Delete" label="Delete" variant="danger">
          <Trash2 size={14} />
        </IconButton>
        <IconButton tooltip="More" label="More" size="sm">
          <MoreHorizontal size={12} />
        </IconButton>
        <IconButton label="Disabled" disabled><Pencil size={14} /></IconButton>
      </div>
    </article>

    <article class="demo">
      <h3>CopyButton</h3>
      <div class="row">
        <CopyButton text="hello from the showcase" />
        <span class="hint">click to copy; swaps to Copied for 1.5s</span>
      </div>
    </article>

    <article class="demo">
      <h3>TypingDots</h3>
      <div class="row">
        <TypingDots />
        <TypingDots label="Assistant is thinking" />
      </div>
    </article>

    <article class="demo">
      <h3>Kbd</h3>
      <div class="row">
        <Kbd>⌘</Kbd>
        <Kbd>⇧</Kbd>
        <Kbd>↵</Kbd>
        <Kbd>Esc</Kbd>
        <span class="inline-kbd">
          Press <Kbd>⌘</Kbd> + <Kbd>K</Kbd> to open.
        </span>
      </div>
    </article>

    <article class="demo">
      <h3>Divider</h3>
      <div class="col" style="gap:12px; width:100%;">
        <Divider />
        <Divider label="Today" />
        <Divider label="Earlier this week" />
      </div>
    </article>

    <article class="demo">
      <h3>InlineCode</h3>
      <p class="paragraph">
        Use the <InlineCode>useChat</InlineCode> hook from the
        <InlineCode>ai-sdk/react</InlineCode> package, then render each message's <InlineCode
          >parts</InlineCode
        > array.
      </p>
    </article>

    <article class="demo">
      <h3>Chip</h3>
      <div class="row">
        <Chip>Plain</Chip>
        <Chip variant="accent">Accent</Chip>
        <Chip variant="muted">Muted</Chip>
        <Chip onremove={() => {}}>Removable</Chip>
        <Chip variant="accent" onremove={() => {}}>accent + ✕</Chip>
      </div>
    </article>

    <article class="demo">
      <h3>SlashToken</h3>
      <div class="row">
        <SlashToken command="help" />
        <SlashToken command="/clear" />
        <SlashToken command="model" />
        <SlashToken command="settings" />
      </div>
    </article>

    <article class="demo">
      <h3>FilePath</h3>
      <div class="row">
        <FilePath path="src/renderer/src/App.svelte" />
        <FilePath path="src/main/terminal.ts" startLine={48} />
        <FilePath
          path="src/renderer/src/components/chat/molecules/ToolCallBlock.svelte"
          startLine={12}
          endLine={48}
        />
        <FilePath path="README.md" showIcon={false} />
      </div>
    </article>

    <article class="demo">
      <h3>SubAgentBadge</h3>
      <div class="row">
        <SubAgentBadge type="Explore" />
        <SubAgentBadge type="Plan" />
        <SubAgentBadge type="general-purpose" />
        <SubAgentBadge type="security-auditor" />
        <SubAgentBadge type="Explore" showIcon={false} />
      </div>
    </article>
  </section>

  <!-- ─────────────────────── MOLECULES ─────────────────────── -->
  <section class="section">
    <h2>Molecules</h2>

    <article class="demo">
      <h3>MessageHeader</h3>
      <div class="col">
        <MessageHeader role="user" userInitial="D" timestamp={now - 120_000} />
        <MessageHeader
          role="assistant"
          brand="ClaudeAI"
          model="claude-opus-4-7"
          timestamp={now - 60_000}
        />
        <MessageHeader role="tool" label="rg" timestamp={now - 30_000} />
        <MessageHeader role="system" timestamp={now} />
      </div>
    </article>

    <article class="demo">
      <h3>MessageActions</h3>
      <p class="hint">Hover the bubble below — actions appear via the hover selector.</p>
      <MessageBubble role="assistant">
        {#snippet header()}
          <MessageHeader
            role="assistant"
            brand="ClaudeAI"
            model="claude-opus-4-7"
            timestamp={now - 120_000}
          />
        {/snippet}
        {#snippet body()}
          Hover me to reveal the action bar in the top-right corner.
        {/snippet}
        {#snippet actions()}
          <MessageActions>
            <CopyButton text="message body" size={12} />
            <IconButton tooltip="Retry" size="sm" label="Retry">
              <RotateCcw size={12} />
            </IconButton>
            <IconButton tooltip="Edit" size="sm" label="Edit">
              <Pencil size={12} />
            </IconButton>
            <IconButton tooltip="Delete" variant="danger" size="sm" label="Delete">
              <Trash2 size={12} />
            </IconButton>
          </MessageActions>
        {/snippet}
      </MessageBubble>

      <p class="hint">Always-visible variant:</p>
      <MessageActions alwaysVisible>
        <CopyButton text="x" size={12} />
        <IconButton tooltip="Retry" size="sm" label="Retry">
          <RotateCcw size={12} />
        </IconButton>
      </MessageActions>
    </article>

    <article class="demo">
      <h3>MessageMeta</h3>
      <div class="col">
        <MessageMeta model="claude-opus-4-7" tokens={1234} elapsedMs={2_140} />
        <MessageMeta model="gpt-5" tokens={42_000} />
        <MessageMeta tokens={180_000} elapsedMs={45_200} />
        <MessageMeta elapsedMs={620} />
      </div>
    </article>

    <article class="demo">
      <h3>MessageBubble</h3>
      <div class="col" style="gap:10px;">
        <MessageBubble role="user">
          {#snippet header()}
            <MessageHeader role="user" userInitial="D" timestamp={now - 240_000} />
          {/snippet}
          {#snippet body()}
            Can you refactor the chat input so it supports multi-line and attachments?
          {/snippet}
        </MessageBubble>

        <MessageBubble role="assistant">
          {#snippet header()}
            <MessageHeader
              role="assistant"
              brand="ClaudeAI"
              model="claude-opus-4-7"
              timestamp={now - 180_000}
            />
          {/snippet}
          {#snippet body()}
            Sure — I'll introduce a <InlineCode>ChatInput</InlineCode> molecule with an auto-growing textarea,
            attachment tray, and inline model picker. Sketch below:
          {/snippet}
          {#snippet footer()}
            <MessageMeta model="claude-opus-4-7" tokens={1832} elapsedMs={3_420} />
          {/snippet}
        </MessageBubble>

        <MessageBubble role="tool">
          {#snippet header()}
            <MessageHeader role="tool" label="read_file" />
          {/snippet}
          {#snippet body()}
            Dashed-border tool bubble — usually collapsed behind a ToolCallBlock.
          {/snippet}
        </MessageBubble>

        <MessageBubble role="system">
          {#snippet body()}
            System messages use the warning accent to stand out.
          {/snippet}
        </MessageBubble>
      </div>
    </article>

    <article class="demo">
      <h3>CodeBlock</h3>
      <CodeBlock code={sampleCode} language="typescript" />
      <CodeBlock code={sampleCode} filename="src/lib/greet.ts" />
    </article>

    <article class="demo">
      <h3>ToolCallBlock — generic fallback</h3>
      <p class="hint">
        Unknown tool names fall back to JSON-dumped <InlineCode>input</InlineCode>
        plus raw <InlineCode>result</InlineCode>.
      </p>
      <ToolCallBlock name="custom_tool" status="running" args={toolArgs} />
      <ToolCallBlock name="custom_tool" status="success" args={toolArgs} result={toolResult} />
      <ToolCallBlock
        name="unknown_api_call"
        status="error"
        input={{ endpoint: '/v1/users', method: 'POST', retries: 3 }}
        result="500 Internal Server Error"
        defaultOpen
      />
    </article>

    <article class="demo">
      <h3>ToolCallBlock — known tools</h3>
      <p class="hint">
        Pass <InlineCode>input</InlineCode> (structured) — the registry picks the right view and shows
        a short summary in the header (path / command / pattern), so you usually don't need to open the
        accordion. Click any header to expand. The first Edit below is opened to show the body view.
      </p>

      <h4 class="subhead">Edit — path + unified diff</h4>
      <ToolCallBlock
        name="Edit"
        status="success"
        input={{
          file_path: 'src/renderer/src/lib/greet.ts',
          old_string: editOldString,
          new_string: editNewString,
        }}
        defaultOpen
      />
      <ToolCallBlock
        name="Edit"
        status="success"
        input={{
          file_path: 'src/main/auth/middleware.ts',
          old_string: 'jwt.verify(token, secret)',
          new_string: "jwt.verify(token, secret, { algorithms: ['HS256'] })",
        }}
      />

      <h4 class="subhead">Write — new content only</h4>
      <ToolCallBlock
        name="Write"
        status="success"
        input={{ file_path: 'src/lib/slugify.ts', content: writeContent }}
        result="File created (87 bytes)"
      />

      <h4 class="subhead">Read — path with line range</h4>
      <ToolCallBlock
        name="Read"
        status="success"
        input={{
          file_path: 'src/renderer/src/main.ts',
          offset: 12,
          limit: 9,
        }}
        result={readResult}
      />

      <h4 class="subhead">Bash — shell prompt + output</h4>
      <ToolCallBlock
        name="Bash"
        status="success"
        input={{
          command: 'ls -lh src/renderer-shared/styles/',
          description: 'List token and style files',
          cwd: '/Users/nix/canopy',
        }}
        result={bashResult}
      />
      <ToolCallBlock
        name="Bash"
        status="error"
        input={{ command: 'rm -rf /protected' }}
        result={bashErrorResult}
      />

      <h4 class="subhead">Grep — pattern + match count</h4>
      <ToolCallBlock
        name="Grep"
        status="success"
        input={{
          pattern: 'TODO',
          path: 'src/',
          output_mode: 'content',
        }}
        result={grepResult}
      />

      <h4 class="subhead">Glob — file pattern</h4>
      <ToolCallBlock
        name="Glob"
        status="success"
        input={{ pattern: 'src/renderer/src/components/chat/**/*.svelte' }}
        result={globResult}
      />

      <h4 class="subhead">WebSearch — query + results</h4>
      <ToolCallBlock
        name="WebSearch"
        status="success"
        input={{ query: 'svelte 5 runes $derived vs $effect' }}
        result={webSearchResult}
      />

      <h4 class="subhead">Edit running — mid-flight</h4>
      <ToolCallBlock
        name="Edit"
        status="running"
        input={{
          file_path: 'src/renderer/src/App.svelte',
          old_string: 'const old = 1',
          new_string: 'const replacement = 1',
        }}
      />
    </article>

    <article class="demo">
      <h3>ThinkingBlock</h3>
      <ThinkingBlock content={thinkingContent} durationMs={820} />
      <ThinkingBlock content={thinkingContent} durationMs={5_420} defaultOpen />
    </article>

    <article class="demo">
      <h3>AttachmentChip</h3>
      <div class="row">
        <AttachmentChip name="screenshot.png" sizeBytes={204_800} kind="image" />
        <AttachmentChip name="notes.md" sizeBytes={1_240} kind="text" />
        <AttachmentChip name="report-final-v3.pdf" sizeBytes={2_400_000} kind="file" />
        <AttachmentChip name="no-size.bin" />
        <AttachmentChip name="removable.png" sizeBytes={12_000} kind="image" onremove={() => {}} />
      </div>
    </article>

    <article class="demo">
      <h3>AttachmentTray</h3>
      <p class="hint">Click × to remove items (state is live).</p>
      <AttachmentTray>
        {#each attachments as att (att.id)}
          <AttachmentChip
            name={att.name}
            sizeBytes={att.size}
            kind={att.kind}
            onremove={() => removeAttachment(att.id)}
          />
        {/each}
        {#if attachments.length === 0}
          <span class="hint">Tray empty — refresh to reset.</span>
        {/if}
      </AttachmentTray>
    </article>

    <article class="demo">
      <h3>SlashCommandHint</h3>
      <div class="hints-list">
        <SlashCommandHint command="help" description="Show help for commands" />
        <SlashCommandHint command="clear" description="Clear the current conversation" focused />
        <SlashCommandHint command="model" description="Switch the active model" />
        <SlashCommandHint command="settings" />
      </div>
    </article>

    <article class="demo">
      <h3>SendControl</h3>
      <div class="row">
        <SendControl onsend={() => {}} />
        <SendControl onsend={() => {}} disabled />
        <SendControl onsend={() => {}} showHint={false} />
      </div>
    </article>

    <article class="demo">
      <h3>ModelPickerInline</h3>
      <div class="row">
        <ModelPickerInline
          value={selectedModel}
          options={modelOptions}
          onchange={(v) => (selectedModel = v)}
        />
        <span class="hint">Selected: {selectedModel}</span>
      </div>
    </article>

    <article class="demo">
      <h3>SubAgentRun — accordion with max-height</h3>
      <p class="hint">
        Nested sub-agent context: left-rail, tint, accordion. Body scrolls internally past <InlineCode
          >maxBodyHeight</InlineCode
        > (default 320px). Running state auto-opens and pulses the rail.
      </p>

      <h4 class="subhead">Completed — collapsed by default</h4>
      <SubAgentRun
        agentType="Explore"
        task="Find all call sites of useChat in src/"
        status="success"
        model="claude-haiku-4-5"
        tokens={3_420}
        elapsedMs={2_140}
      >
        {#snippet body()}
          <ToolCallBlock
            name="Grep"
            status="success"
            input={{ pattern: 'useChat', path: 'src/' }}
            result={grepResult}
          />
          <ToolCallBlock
            name="Read"
            status="success"
            input={{ file_path: 'src/renderer/src/App.svelte', offset: 12, limit: 9 }}
            result={readResult}
          />
          <MessageBubble role="assistant">
            {#snippet body()}
              Found 3 call sites. All wrap messages in a
              <InlineCode>useChat</InlineCode> helper with identical API shape.
            {/snippet}
          </MessageBubble>
        {/snippet}
        {#snippet summary()}
          <p>
            <strong>3 files</strong>: App.svelte, Chat.svelte, Demo.svelte. All follow the same
            pattern — safe to refactor.
          </p>
        {/snippet}
      </SubAgentRun>

      <h4 class="subhead">Running — auto-open, animated rail</h4>
      <SubAgentRun
        agentType="Plan"
        task="Design migration from REST polling to WebSocket subscriptions"
        status="running"
        model="claude-opus-4-7"
        elapsedMs={12_400}
      >
        {#snippet body()}
          <ThinkingBlock content={thinkingContent} durationMs={4_200} />
          <ToolCallBlock
            name="Read"
            status="success"
            input={{ file_path: 'src/main/sync/poller.ts' }}
            result={readResult}
          />
          <ToolCallBlock
            name="Grep"
            status="running"
            input={{ pattern: 'setInterval', path: 'src/main/' }}
            defaultOpen
          />
        {/snippet}
      </SubAgentRun>

      <h4 class="subhead">Error — red rail, error dot</h4>
      <SubAgentRun
        agentType="security-auditor"
        task="Review recent auth changes for CVE patterns"
        status="error"
        model="claude-opus-4-7"
        tokens={8_100}
        elapsedMs={18_500}
        defaultOpen
      >
        {#snippet body()}
          <ToolCallBlock
            name="Grep"
            status="success"
            input={{ pattern: 'jwt.verify', path: 'src/main/auth/' }}
            result="src/main/auth/middleware.ts:34"
          />
          <MessageBubble role="system">
            {#snippet body()}
              Sub-agent aborted: rate limit reached on upstream model.
            {/snippet}
          </MessageBubble>
        {/snippet}
        {#snippet summary()}
          <p>Failed with rate-limit after 18.5s. Partial findings discarded.</p>
        {/snippet}
      </SubAgentRun>

      <h4 class="subhead">Tight max-height — forces internal scroll</h4>
      <SubAgentRun
        agentType="general-purpose"
        task="Large search: 40+ tool calls, body capped at 180px"
        status="success"
        tokens={24_000}
        elapsedMs={42_000}
        maxBodyHeight={180}
        defaultOpen
      >
        {#snippet body()}
          {#each fileIndices as i (i)}
            <ToolCallBlock
              name="Read"
              status="success"
              input={{ file_path: `src/lib/file-${i + 1}.ts` }}
              result="… content elided …"
            />
          {/each}
        {/snippet}
        {#snippet summary()}
          <p>Scanned 12 files — no regressions found.</p>
        {/snippet}
      </SubAgentRun>

      <h4 class="subhead">Inside an assistant bubble</h4>
      <p class="hint">Common shape: parent assistant spawns a sub-agent mid-reply.</p>
      <MessageBubble role="assistant">
        {#snippet header()}
          <MessageHeader
            role="assistant"
            brand="ClaudeAI"
            model="claude-opus-4-7"
            timestamp={now - 300_000}
          />
        {/snippet}
        {#snippet body()}
          Let me dispatch an Explore agent to understand the layout before I touch any code.

          <SubAgentRun
            agentType="Explore"
            task="Map component directory structure"
            status="success"
            tokens={1_240}
            elapsedMs={1_800}
          >
            {#snippet body()}
              <ToolCallBlock
                name="Glob"
                status="success"
                input={{ pattern: 'src/renderer/src/components/**/*.svelte' }}
                result={globResult}
              />
            {/snippet}
            {#snippet summary()}
              <p>27 components across 14 folders; chat/ is clean atomic layout.</p>
            {/snippet}
          </SubAgentRun>

          With that context, here's the plan: …
        {/snippet}
      </MessageBubble>
    </article>

    <article class="demo">
      <h3>ChatInput — full composition</h3>
      <p class="hint">
        Type a message and press <Kbd>↵</Kbd> or <Kbd>⌘</Kbd>+<Kbd>↵</Kbd>. Recent submissions
        appear below. <Kbd>⇧</Kbd>+<Kbd>↵</Kbd> inserts a newline.
      </p>

      <ChatInput
        bind:value={chatValue}
        placeholder="Ask anything…"
        onsubmit={handleSubmit}
        onattach={() => {}}
      >
        {#snippet attachments()}
          <AttachmentTray>
            <AttachmentChip name="context.ts" sizeBytes={3_210} kind="text" onremove={() => {}} />
            <AttachmentChip
              name="design.png"
              sizeBytes={120_000}
              kind="image"
              onremove={() => {}}
            />
          </AttachmentTray>
        {/snippet}
        {#snippet modelPicker()}
          <ModelPickerInline
            value={selectedModel}
            options={modelOptions}
            onchange={(v) => (selectedModel = v)}
          />
        {/snippet}
        {#snippet commandHints()}
          <div class="hints-list">
            <SlashCommandHint command="help" description="Show help" focused />
            <SlashCommandHint command="clear" description="Clear conversation" />
          </div>
        {/snippet}
      </ChatInput>

      {#if submissions.length > 0}
        <div class="submissions">
          <Divider label="Submissions" />
          {#each submissions as s, i (i + '-' + s)}
            <div class="submission">{s}</div>
          {/each}
        </div>
      {/if}
    </article>
  </section>

  <!-- ────────────────────── USER ATTENTION ───────────────────── -->
  <section class="section">
    <h2>User Attention</h2>

    <article class="demo">
      <h3>AttentionBanner (shell)</h3>
      <AttentionBanner title="Waiting for input" tone="warning">
        {#snippet description()}
          Generic attention shell — used by the blocks below.
        {/snippet}
        {#snippet body()}
          <p class="paragraph">Any content can sit inside the body snippet.</p>
        {/snippet}
        {#snippet actions()}
          <button type="button" class="demo-btn">Act</button>
        {/snippet}
      </AttentionBanner>
      <AttentionBanner title="Approved" tone="accent" status="resolved">
        {#snippet description()}
          Green rail + tone override when status is resolved.
        {/snippet}
      </AttentionBanner>
      <AttentionBanner title="Rejected" tone="warning" status="rejected">
        {#snippet description()}
          Red rail + tone override when status is rejected.
        {/snippet}
      </AttentionBanner>
    </article>

    <article class="demo">
      <h3>QuestionOption</h3>
      <div class="col">
        <QuestionOption label="Basic option" description="Single-select, unchecked." />
        <QuestionOption label="Selected option" description="Single-select, checked." selected />
        <QuestionOption
          label="Recommended option (Recommended)"
          description="Badge is auto-parsed from the label suffix."
          selected
        />
        <QuestionOption
          label="Multi-select"
          description="Uses CustomCheckbox instead of CustomRadio."
          multiSelect
          selected
        />
        <QuestionOption label="Disabled" description="Read-only state." disabled />
      </div>
    </article>

    <article class="demo">
      <h3>QuestionBlock — single-select</h3>
      <QuestionBlock
        header={libraryQuestion.header}
        question={libraryQuestion.question}
        options={libraryQuestion.options}
      />
    </article>

    <article class="demo">
      <h3>QuestionBlock — multi-select</h3>
      <QuestionBlock
        header={featuresQuestion.header}
        question={featuresQuestion.question}
        options={featuresQuestion.options}
        multiSelect
      />
    </article>

    <article class="demo">
      <h3>QuestionBlock — side-by-side preview</h3>
      <QuestionBlock
        header={approachQuestion.header}
        question={approachQuestion.question}
        options={approachQuestion.options}
      />
    </article>

    <article class="demo">
      <h3>QuestionnaireBlock — waiting</h3>
      <QuestionnaireBlock
        questions={[libraryQuestion, featuresQuestion]}
        onsubmit={(answers) => (submittedAnswers = answers)}
      />
      {#if submittedAnswers}
        <div class="submissions">
          <Divider label="Submitted" />
          <pre class="submission">{JSON.stringify(submittedAnswers, null, 2)}</pre>
        </div>
      {/if}
    </article>

    <article class="demo">
      <h3>QuestionnaireBlock — resolved (read-only)</h3>
      <QuestionnaireBlock
        questions={[libraryQuestion]}
        status="resolved"
        answers={{
          [libraryQuestion.question]: {
            selected: ['date-fns (Recommended)'],
          },
        }}
      />
    </article>

    <article class="demo">
      <h3>QuestionnaireBlock — submitting / rejected</h3>
      <QuestionnaireBlock questions={[libraryQuestion]} status="submitting" />
      <QuestionnaireBlock questions={[libraryQuestion]} status="rejected" />
    </article>

    <article class="demo">
      <h3>PlanApprovalBlock — waiting</h3>
      <PlanApprovalBlock allowedPrompts={planPermissions}>
        {#snippet plan()}
          <p>
            <strong>Summary.</strong> Swap out the 3-dot bouncer for a Braille cli-spinners style animation.
          </p>
          <p>
            Rewrite the TypingDots atom to tick through 10 Braille frames at 80 ms each, respecting
            <code>prefers-reduced-motion</code>.
          </p>
          <ul>
            <li>
              Files: <code>src/renderer/src/components/chat/atoms/TypingDots.svelte</code>
            </li>
            <li>
              Verify: <code>npm run svelte-check</code> + manual eyeball at <code>/showcase</code>
            </li>
          </ul>
        {/snippet}
      </PlanApprovalBlock>
    </article>

    <article class="demo">
      <h3>PlanApprovalBlock — approved</h3>
      <PlanApprovalBlock allowedPrompts={planPermissions} status="approved">
        {#snippet plan()}
          <p>{samplePlan}</p>
        {/snippet}
      </PlanApprovalBlock>
    </article>

    <article class="demo">
      <h3>PlanApprovalBlock — rejected with feedback</h3>
      <PlanApprovalBlock
        allowedPrompts={planPermissions}
        status="rejected"
        feedback="Please also add a test case covering the prefers-reduced-motion branch."
      >
        {#snippet plan()}
          <p>{samplePlan}</p>
        {/snippet}
      </PlanApprovalBlock>
    </article>

    <article class="demo">
      <h3>ToolPermissionBlock — Edit (file-path summary)</h3>
      <ToolPermissionBlock
        tool="Edit"
        input={{ file_path: 'src/renderer/src/components/chat/atoms/TypingDots.svelte' }}
        reason="Rewriting the spinner as documented in the plan."
        onrespond={(d) => logPermission('Edit', d)}
      />
    </article>

    <article class="demo">
      <h3>ToolPermissionBlock — Bash (command summary)</h3>
      <ToolPermissionBlock
        tool="Bash"
        input={{ command: 'npm run svelte-check' }}
        reason="Verify no new type errors before declaring done."
        onrespond={(d) => logPermission('Bash', d)}
      />
    </article>

    <article class="demo">
      <h3>ToolPermissionBlock — unknown tool fallback</h3>
      <ToolPermissionBlock
        tool="CustomTool"
        input={{ param1: 'value', param2: 42, nested: { flag: true } }}
        onrespond={(d) => logPermission('CustomTool', d)}
      />
    </article>

    <article class="demo">
      <h3>ToolPermissionBlock — resolved states</h3>
      <ToolPermissionBlock
        tool="Edit"
        input={{ file_path: 'src/App.svelte' }}
        status="granted"
        priorDecision="allow-session"
      />
      <ToolPermissionBlock
        tool="Bash"
        input={{ command: 'rm -rf node_modules' }}
        status="denied"
        priorDecision="deny"
      />
      <ToolPermissionBlock
        tool="WebSearch"
        input={{ query: 'Svelte 5 runes' }}
        status="submitting"
      />
    </article>

    {#if permissionLog.length > 0}
      <article class="demo">
        <h3>Permission log</h3>
        <div class="submissions">
          {#each permissionLog as entry, i (i + '-' + entry)}
            <div class="submission">{entry}</div>
          {/each}
        </div>
      </article>
    {/if}
  </section>

  <footer class="page-footer">
    <span>
      <TypingDots /> 35 components: 16 atoms + 19 molecules
    </span>
  </footer>
</div>

<style>
  :global(html) {
    height: auto;
    overflow: auto;
  }

  :global(body) {
    height: auto;
    overflow: auto;
  }

  :global(#app) {
    height: auto;
    min-height: 100vh;
    overflow: visible;
  }

  .showcase {
    max-width: 960px;
    margin: 0 auto;
    padding: 40px 32px 80px;
    color: var(--c-text);
    font-size: 13.5px;
    line-height: 1.6;
  }

  .page-header {
    margin-bottom: 32px;
  }

  .page-header h1 {
    margin: 0 0 6px;
    font-size: 22px;
    font-weight: 600;
  }

  .subtitle {
    color: var(--c-text-secondary);
    font-size: 13px;
    margin: 0;
  }

  .subtitle code {
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    color: var(--c-accent-text);
  }

  .section {
    margin-top: 36px;
    padding-top: 24px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .section > h2 {
    margin: 0 0 20px;
    font-size: 16px;
    font-weight: 600;
    color: var(--c-text);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .demo {
    padding: 16px 18px;
    margin-bottom: 14px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 8px;
    background: color-mix(in srgb, var(--c-bg-elevated) 35%, transparent);
  }

  .demo h3 {
    margin: 0 0 12px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-text-muted);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-start;
  }

  .stack {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .caption {
    font-size: 10.5px;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .hint {
    font-size: 11.5px;
    color: var(--c-text-muted);
  }

  .subhead {
    margin: 14px 0 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-secondary);
    text-transform: none;
    letter-spacing: 0;
  }

  .subhead:first-child {
    margin-top: 0;
  }

  .inline-kbd {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
    color: var(--c-text-secondary);
  }

  .paragraph {
    margin: 0;
    color: var(--c-text);
    font-size: 13.5px;
  }

  .hints-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    max-width: 360px;
  }

  .submissions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
  }

  .submission {
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    font-size: 12.5px;
    white-space: pre-wrap;
    -webkit-user-select: text;
    user-select: text;
  }

  .page-footer {
    margin-top: 40px;
    padding-top: 16px;
    border-top: 1px solid var(--c-border-subtle);
    color: var(--c-text-muted);
    font-size: 11.5px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
