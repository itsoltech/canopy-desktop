/* eslint-disable @typescript-eslint/no-unused-vars */
/**
 * Canopy bridge plugin for OpenCode.
 *
 * Runs inside the OpenCode process, subscribes to lifecycle events,
 * and forwards them as JSON POSTs to Canopy's agent hook HTTP server.
 *
 * Environment variables (set by Canopy before spawning OpenCode):
 *   CANOPY_HOOK_PORT  — localhost port of the hook server
 *   CANOPY_HOOK_TOKEN — per-session auth token
 *   CANOPY_HOOK_PATH  — URL path prefix (e.g. /session/<id>)
 *
 * When env vars are absent (OpenCode launched outside Canopy), the plugin
 * returns an empty hooks object and does nothing.
 */

const port = process.env.CANOPY_HOOK_PORT
const token = process.env.CANOPY_HOOK_TOKEN
const basePath = process.env.CANOPY_HOOK_PATH || ''

async function postHook(payload: Record<string, unknown>): Promise<void> {
  if (!port || !token) return
  if (!/^\d+$/.test(port)) return
  try {
    const url = new URL(`http://127.0.0.1:${port}${basePath}/hook`)
    await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Canopy-Auth': token,
      },
      body: JSON.stringify(payload),
    })
  } catch {
    // Fire-and-forget — Canopy may have shut down
  }
}

// Plugin matches the Plugin type: (input: PluginInput, options?: PluginOptions) => Promise<Hooks>
// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
export const CanopyBridge = async (_input: unknown, _options?: unknown) => {
  if (!port || !token) return {}

  return {
    // Generic event handler — receives all OpenCode events as a discriminated union
    event: async (input: { event: { type: string; properties?: Record<string, unknown> } }) => {
      const { type, properties } = input.event
      const sessionID = (properties?.sessionID as string) ?? ''

      switch (type) {
        case 'session.created': {
          const info = properties?.info as Record<string, unknown> | undefined
          await postHook({
            hook_event_name: 'SessionCreated',
            session_id: info?.id ?? sessionID,
          })
          break
        }

        case 'session.idle':
          await postHook({
            hook_event_name: 'SessionIdle',
            session_id: sessionID,
          })
          break

        case 'session.error':
          await postHook({
            hook_event_name: 'SessionError',
            session_id: sessionID,
            error:
              typeof properties?.error === 'string'
                ? properties.error
                : JSON.stringify(properties?.error ?? 'unknown error'),
          })
          break

        case 'session.deleted':
          await postHook({
            hook_event_name: 'SessionDeleted',
            session_id: sessionID,
            reason: 'deleted',
          })
          break

        case 'session.compacted':
          await postHook({
            hook_event_name: 'SessionCompacted',
            session_id: sessionID,
          })
          break

        case 'session.status': {
          const status = properties?.status as Record<string, unknown> | undefined
          const statusType = status?.type as string | undefined
          // Split into distinct event names so Canopy can track busy/idle state
          await postHook({
            hook_event_name: statusType === 'busy' ? 'SessionBusy' : 'SessionStatusIdle',
            session_id: sessionID,
            status: statusType,
          })
          break
        }

        case 'todo.updated': {
          const todos = properties?.todos as Array<Record<string, unknown>> | undefined
          await postHook({
            hook_event_name: 'TodoUpdated',
            session_id: sessionID,
            notification_type: 'Todo',
            title: 'Task updated',
            todos,
          })
          break
        }

        case 'permission.updated': {
          // Permission events come through as events too
          await postHook({
            hook_event_name: 'PermissionAsked',
            session_id: sessionID,
            tool_name: (properties as Record<string, unknown>)?.title ?? '',
            tool_input: (properties as Record<string, unknown>)?.metadata,
          })
          break
        }
      }
    },

    // Tool execution hooks — specific interceptors with typed signatures
    'tool.execute.before': async (
      input: { tool: string; sessionID: string; callID: string },
      output: { args: unknown },
    ) => {
      await postHook({
        hook_event_name: 'ToolExecuteBefore',
        session_id: input.sessionID,
        tool_name: input.tool,
        tool_input: typeof output.args === 'object' ? output.args : { raw: output.args },
      })
    },

    'tool.execute.after': async (
      input: { tool: string; sessionID: string; callID: string; args: unknown },
      output: { title: string; output: string; metadata: unknown },
    ) => {
      await postHook({
        hook_event_name: 'ToolExecuteAfter',
        session_id: input.sessionID,
        tool_name: input.tool,
        tool_input: typeof input.args === 'object' ? input.args : { raw: input.args },
        tool_response: output.output,
      })
    },

    // Permission ask hook
    'permission.ask': async (
      input: { id: string; type: string; sessionID: string; title: string; metadata: unknown },
      _output: { status: string },
    ) => {
      await postHook({
        hook_event_name: 'PermissionAsked',
        session_id: input.sessionID,
        tool_name: input.title,
        tool_input: typeof input.metadata === 'object' ? input.metadata : undefined,
      })
    },

    // Compaction start hook
    'experimental.session.compacting': async (input: { sessionID: string }, _output: unknown) => {
      await postHook({
        hook_event_name: 'SessionCompacting',
        session_id: input.sessionID,
      })
    },
  }
}
