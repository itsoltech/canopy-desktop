# Assets

- `fonts/JetBrainsMono-Regular.ttf`: copied from the Electron Canopy reference repository;
  JetBrains Mono, SIL Open Font License 1.1, see `fonts/OFL.txt`.
- `icons/git-branch.svg`, `code.svg`, `terminal.svg`: Lucide, downloaded from
  https://github.com/lucide-icons/lucide/tree/main/icons on 2026-09-08;
  license in `icons/LICENSE-lucide`.
- `icons/claude.svg`, `openai.svg`, `gemini.svg`: SVGL project, downloaded from
  https://github.com/pheralb/svgl/tree/main/static/library on 2026-09-08
  (claude-ai-icon.svg, openai.svg, gemini.svg). Repository license in
  `icons/LICENSE-svgl`; brand marks remain the property of their owners.
- Other icons: GPUI Kit bundled assets, resolved by Cargo.lock.

All assets are embedded at build time; the running UI makes no asset requests.

Additional Preferences Lucide icons (same upstream and license): shield,
keyboard, diamond, braces, wrench, sparkles, download, upload.

`icons/jira.svg` is the CC0 Simple Icons Jira mark, reused from the Electron Canopy reference.
