export type AiError = { _tag: 'AiRequestFailed'; message: string } | { _tag: 'NoOutput' }
