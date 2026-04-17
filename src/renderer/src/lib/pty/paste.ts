// eslint-disable-next-line no-control-regex
const BRACKETED_PASTE_END = /\x1b\[201~/g

/**
 * Strip C0 control characters (keep \n 0x0A and \t 0x09) and ANSI CSI
 * sequences. Used before piping arbitrary text into a PTY so stray control
 * bytes can't hijack the terminal.
 */
export function sanitizePtyInput(text: string): string {
  let result = ''
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i)
    if (code === 0x1b && text[i + 1] === '[') {
      let j = i + 2
      while (j < text.length && !/[a-zA-Z]/.test(text[j])) j++
      i = j
      continue
    }
    if (code < 0x20 && code !== 0x0a && code !== 0x09) continue
    if (code === 0x7f) continue
    result += text[i]
  }
  return result
}

/**
 * Wrap text in bracketed-paste markers so the CLI on the other end of the
 * PTY treats multi-chunk arrivals (e.g. ConPTY splitting a large write)
 * as one paste instead of firing its line-at-a-time / timing-based input
 * handlers on the fragments.
 *
 * Any stray `\x1b[201~` inside the content is stripped first — it would
 * otherwise terminate the paste early, and task descriptions can contain
 * arbitrary terminal transcripts.
 */
export function wrapAsBracketedPaste(text: string): string {
  const sanitised = sanitizePtyInput(text).replace(BRACKETED_PASTE_END, '')
  return `\x1b[200~${sanitised}\x1b[201~`
}
