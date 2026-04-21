import { BrowserWindow, ipcMain } from 'electron'
import type { SdkAgentManager } from './SdkAgentManager'
import type {
  AskUserQuestionAnswer,
  Attachment,
  AttachmentKind,
  ConversationId,
  PermissionMode,
  PlanDecision,
  SdkAgentEvent,
  ToolDecision,
} from './types'
import { asConversationId } from './types'
import { uploadAttachment, type UploadAttachmentError } from './attachmentPipeline'

function eventChannel(id: ConversationId): string {
  return `sdkAgent:event:${id}`
}

/**
 * Broadcast every SDK-agent event to every live window. Individual panes
 * attach listeners on their own channel (`sdkAgent:event:<id>`), so windows
 * that don't care about a given conversation never see its events.
 */
function broadcast(id: ConversationId, event: SdkAgentEvent): void {
  const channel = eventChannel(id)
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) win.webContents.send(channel, event)
  }
}

export function registerSdkAgentIpcHandlers(manager: SdkAgentManager): void {
  // Fan-out bridge — any session created via this handler automatically
  // broadcasts its events. Phase 5's renderer store will `.on(channel)` to
  // consume them.
  const installedFor = new Set<string>()
  function installBridge(id: ConversationId): void {
    if (installedFor.has(id)) return
    installedFor.add(id)
    manager.subscribe(id, (event) => broadcast(id, event))
  }

  ipcMain.handle(
    'sdkAgent:create',
    async (
      _e,
      args: { workspaceId: string; worktreePath: string; profileId: string },
    ): Promise<{ conversationId: ConversationId } | { error: string }> => {
      const result = await manager.createSession({
        workspaceId: args.workspaceId,
        worktreePath: args.worktreePath,
        agentProfileId: args.profileId,
      })
      if ('error' in result) return { error: result.error._tag }
      installBridge(result.conversationId)
      return { conversationId: result.conversationId }
    },
  )

  ipcMain.handle(
    'sdkAgent:send',
    async (
      _e,
      args: {
        conversationId: string
        text: string
        attachments?: Attachment[]
        modelOverride?: string
        permissionModeOverride?: PermissionMode
      },
    ): Promise<{ ok: true } | { error: string }> => {
      const id = asConversationId(args.conversationId)
      installBridge(id)
      const result = await manager.sendMessage({
        conversationId: id,
        text: args.text,
        attachments: args.attachments,
        modelOverride: args.modelOverride,
        permissionModeOverride: args.permissionModeOverride,
      })
      if (result && 'error' in result) return { error: result.error._tag }
      return { ok: true }
    },
  )

  ipcMain.handle('sdkAgent:cancel', (_e, conversationId: string) => {
    manager.cancel(asConversationId(conversationId))
  })

  ipcMain.handle('sdkAgent:close', (_e, conversationId: string) => {
    manager.closeSession(asConversationId(conversationId))
  })

  ipcMain.handle('sdkAgent:delete', (_e, conversationId: string) => {
    manager.deleteConversation(asConversationId(conversationId))
  })

  ipcMain.handle('sdkAgent:list', (_e, workspaceId: string) =>
    manager.listConversations(workspaceId),
  )

  ipcMain.handle('sdkAgent:getTranscript', (_e, conversationId: string) => {
    const id = asConversationId(conversationId)
    installBridge(id)
    return manager.getTranscript(id)
  })

  ipcMain.handle(
    'sdkAgent:search',
    (_e, args: { workspaceId: string; query: string; limit?: number }) =>
      manager.searchConversations(args.workspaceId, args.query, args.limit ?? 50),
  )

  ipcMain.handle(
    'sdkAgent:respondPermission',
    (_e, args: { conversationId: string; requestId: string; decision: ToolDecision }) => {
      manager.respondPermission(
        asConversationId(args.conversationId),
        args.requestId,
        args.decision,
      )
    },
  )

  ipcMain.handle(
    'sdkAgent:respondQuestion',
    (
      _e,
      args: {
        conversationId: string
        requestId: string
        answers: Record<string, AskUserQuestionAnswer>
      },
    ) => {
      manager.respondQuestion(asConversationId(args.conversationId), args.requestId, args.answers)
    },
  )

  ipcMain.handle(
    'sdkAgent:respondPlan',
    (_e, args: { conversationId: string; requestId: string; decision: PlanDecision }) => {
      manager.respondPlan(asConversationId(args.conversationId), args.requestId, args.decision)
    },
  )

  ipcMain.handle(
    'sdkAgent:uploadAttachment',
    (
      _e,
      args: {
        conversationId: string
        filename: string
        mimeType: string
        kind: AttachmentKind
        dataBase64: string
      },
    ): Attachment | { error: UploadAttachmentError } => {
      const result = uploadAttachment({
        conversationId: asConversationId(args.conversationId),
        filename: args.filename,
        mimeType: args.mimeType,
        kind: args.kind,
        dataBase64: args.dataBase64,
      })
      if ('_tag' in result) return { error: result }
      return result
    },
  )
}
