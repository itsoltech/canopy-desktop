// The babel widgets plugin (auto-enabled by babel-preset-expo when expo-widgets
// is installed) serializes the function body below to a string at build time
// because of the 'widget' directive. That string is shipped to the native
// widget runtime and evaluated there at activity start. Consequences:
//  - only the function body is serialized; anything declared outside it is
//    invisible at runtime and must stay inline;
//  - the widget runtime pre-populates globalThis with @expo/ui/swift-ui,
//    modifiers, React and JSX runtime, so those imports resolve as globals
//    inside the serialized body even though the app bundle's imports don't
//    travel with the function.
import { HStack, Image, Text, VStack } from '@expo/ui/swift-ui'
import { font, foregroundStyle, padding } from '@expo/ui/swift-ui/modifiers'
import { createLiveActivity, type LiveActivityLayout } from 'expo-widgets'

import type { AgentActivityProps } from './types'

const CanopyAgentActivityLayout = (props: AgentActivityProps): LiveActivityLayout => {
  'widget'

  const colors = {
    idle: '#30d158',
    working: '#0a84ff',
    waiting: '#ffd60a',
    error: '#ff453a',
  }
  const colorFor = (s: string): string => {
    if (s === 'working') return colors.working
    if (s === 'waiting') return colors.waiting
    if (s === 'error') return colors.error
    return colors.idle
  }
  const headline = (): string => {
    const n = props.workingCount + props.waitingCount + props.errorCount
    if (n === 0) return 'Canopy idle'
    const plural = n === 1 ? '' : 's'
    if (props.overallStatus === 'error') return `${n} agent${plural} errored`
    if (props.overallStatus === 'waiting') return `${n} agent${plural} awaiting permission`
    if (props.overallStatus === 'working') return `${n} agent${plural} working`
    return 'Canopy idle'
  }
  const row = (branch: string, status: string): React.JSX.Element => {
    if (branch === '' || status === '') {
      return <Text modifiers={[font({ size: 1 })]}> </Text>
    }
    return (
      <HStack spacing={8}>
        <Image
          systemName="circle.fill"
          modifiers={[foregroundStyle(colorFor(status)), font({ size: 10 })]}
        />
        <Text modifiers={[font({ size: 13 })]}>{branch}</Text>
      </HStack>
    )
  }

  const tint = colorFor(props.overallStatus)
  const totalActive = props.workingCount + props.waitingCount + props.errorCount

  return {
    banner: (
      <VStack spacing={6} modifiers={[padding({ all: 12 })]}>
        <HStack spacing={8}>
          <Image systemName="circle.fill" modifiers={[foregroundStyle(tint), font({ size: 12 })]} />
          <Text modifiers={[font({ weight: 'semibold', size: 14 })]}>{headline()}</Text>
        </HStack>
        {row(props.top1Branch, props.top1Status)}
        {row(props.top2Branch, props.top2Status)}
        {row(props.top3Branch, props.top3Status)}
      </VStack>
    ),
    compactLeading: <Image systemName="circle.fill" modifiers={[foregroundStyle(tint)]} />,
    compactTrailing: (
      <Text modifiers={[font({ size: 13, weight: 'semibold' })]}>{String(totalActive)}</Text>
    ),
    minimal: <Image systemName="circle.fill" modifiers={[foregroundStyle(tint)]} />,
    expandedLeading: (
      <VStack spacing={2} modifiers={[padding({ all: 8 })]}>
        <Image systemName="circle.fill" modifiers={[foregroundStyle(tint), font({ size: 18 })]} />
        <Text modifiers={[font({ size: 11 })]}>Canopy</Text>
      </VStack>
    ),
    expandedTrailing: (
      <VStack spacing={2} modifiers={[padding({ all: 8 })]}>
        <Text modifiers={[font({ size: 18, weight: 'semibold' })]}>{String(totalActive)}</Text>
        <Text modifiers={[font({ size: 10 })]}>active</Text>
      </VStack>
    ),
    expandedBottom: (
      <VStack spacing={4} modifiers={[padding({ all: 8 })]}>
        {row(props.top1Branch, props.top1Status)}
        {row(props.top2Branch, props.top2Status)}
        {row(props.top3Branch, props.top3Status)}
      </VStack>
    ),
  }
}

export const CanopyAgentActivity = createLiveActivity<AgentActivityProps>(
  'CanopyAgentActivity',
  CanopyAgentActivityLayout,
)
