import { describe, expect, it } from 'vitest'
import { uniqueAttachmentName } from './attachmentNames'

describe('uniqueAttachmentName', () => {
  it('keeps a name that is not taken yet', () => {
    expect(uniqueAttachmentName('image.png', [])).toBe('image.png')
  })

  it('numbers repeated pasted screenshots before the extension', () => {
    // Chromium names every pasted screenshot "image.png".
    expect(uniqueAttachmentName('image.png', ['image.png'])).toBe('image-2.png')
    expect(uniqueAttachmentName('image.png', ['image.png', 'image-2.png'])).toBe('image-3.png')
  })

  it('handles names without an extension and dotfiles', () => {
    expect(uniqueAttachmentName('notes', ['notes'])).toBe('notes-2')
    expect(uniqueAttachmentName('.env', ['.env'])).toBe('.env-2')
  })
})
