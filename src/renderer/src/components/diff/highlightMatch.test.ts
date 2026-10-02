import { describe, expect, it } from 'vitest'
import { highlightMatch } from './highlightMatch'

describe('highlightMatch', () => {
  it('escapes markup in the diff line', () => {
    expect(highlightMatch('<img src=x onerror=alert(1)>', '')).toBe(
      '&lt;img src=x onerror=alert(1)&gt;',
    )
  })

  it('marks matches without breaking escaped characters around them', () => {
    expect(highlightMatch('if (a < b) return', 'lt')).toBe('if (a &lt; b) return')
    expect(highlightMatch('a && b', 'amp')).toBe('a &amp;&amp; b')
  })

  it('marks every case-insensitive match, including ones containing markup characters', () => {
    expect(highlightMatch('Foo<foo>', 'o<f')).toBe(
      'Fo<mark class="search-highlight">o&lt;f</mark>oo&gt;',
    )
  })
})
