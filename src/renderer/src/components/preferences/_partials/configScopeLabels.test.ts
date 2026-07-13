import { describe, it, expect } from 'vitest'
import {
  trackersNeedingCredentials,
  renderTemplateExample,
  BRANCH_EXAMPLE_VALUES,
  PR_EXAMPLE_VALUES,
  RENDERER_DEFAULT_BRANCH_TEMPLATE,
  RENDERER_DEFAULT_PR_TITLE,
  RENDERER_DEFAULT_PR_BODY,
} from './configScopeLabels'

describe('trackersNeedingCredentials', () => {
  it('returns repo trackers that have no stored credentials', () => {
    const repo = [{ id: 'a' }, { id: 'b' }]
    expect(trackersNeedingCredentials(repo, (id) => id === 'a')).toEqual([{ id: 'b' }])
  })

  it('returns empty when every tracker has credentials', () => {
    expect(trackersNeedingCredentials([{ id: 'a' }, { id: 'b' }], () => true)).toEqual([])
  })

  it('returns empty for no trackers', () => {
    expect(trackersNeedingCredentials([], () => false)).toEqual([])
  })
})

describe('renderTemplateExample', () => {
  it('substitutes known fields with sample values', () => {
    expect(renderTemplateExample(RENDERER_DEFAULT_BRANCH_TEMPLATE, BRANCH_EXAMPLE_VALUES)).toBe(
      'feature/ISSUE-123-fix-login-bug',
    )
    expect(renderTemplateExample(RENDERER_DEFAULT_PR_TITLE, PR_EXAMPLE_VALUES)).toBe(
      '[ISSUE-123] Fix login bug',
    )
  })

  it('leaves unknown fields untouched', () => {
    expect(renderTemplateExample('{nope}/{taskKey}', PR_EXAMPLE_VALUES)).toBe('{nope}/ISSUE-123')
  })
})

describe('default template presets', () => {
  it('expose the built-in seeds with the task key placeholder', () => {
    expect(RENDERER_DEFAULT_BRANCH_TEMPLATE).toContain('{taskKey}')
    expect(RENDERER_DEFAULT_PR_TITLE).toContain('{taskKey}')
    expect(RENDERER_DEFAULT_PR_BODY).toContain('{taskKey}')
  })
})
