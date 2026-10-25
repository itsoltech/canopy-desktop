import React, { useState } from 'react'
import { Pressable, View } from 'react-native'
import { ThemedText } from '@/components/themed-text'
import { Field } from './common'
import { Spacing } from '@/constants/theme'
// Native rendering keeps note content out of a privileged browser. Editing a block
// replaces that exact source range, retaining Markdown that this renderer doesn't style.
export function Markdown({ text, change }: { text: string; change?: (text: string) => void }) {
  const [editing, setEditing] = useState<number>()
  const blocks = text.split(/(\n\s*\n)/)
  return <View style={{ gap: Spacing.two }}>{blocks.map((block, index) => {
    if (!block.trim()) return null
    if (editing === index && change) return <Field key={index} label="Edit Markdown block" multiline autoFocus value={block} onChangeText={value => { const next = [...blocks]; next[index] = value; change(next.join('')) }} onBlur={() => setEditing(undefined)} />
    const code = block.startsWith('```'), heading = /^#{1,6}\s/.test(block)
    const content = code ? block.replace(/^```[^\n]*\n?/, '').replace(/\n?```$/, '') : block.replace(/^#{1,6}\s/, '')
    return <Pressable key={index} accessibilityRole={change ? 'button' : 'text'} accessibilityLabel={change ? 'Edit note block' : undefined} disabled={!change} onPress={() => setEditing(index)}><ThemedText selectable={!change} type={code ? 'code' : heading ? 'subtitle' : 'default'}>{content}</ThemedText></Pressable>
  })}{!text && change && <Field label="Write a note" multiline value={text} onChangeText={change} />}</View>
}
