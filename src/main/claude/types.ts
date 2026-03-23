export interface HookEvent {
  session_id: string
  hook_event_name: string
  transcript_path?: string
  cwd?: string
  permission_mode?: string
  tool_name?: string
  tool_input?: Record<string, unknown>
  tool_response?: string
  error?: string
  error_details?: string
  message?: string
  title?: string
  notification_type?: string
  agent_id?: string
  agent_type?: string
  last_assistant_message?: string
  reason?: string
  source?: string
  model?: string
  stop_hook_active?: boolean
  is_interrupt?: boolean
  task_id?: string
  task_subject?: string
  task_description?: string
  teammate_name?: string
  team_name?: string
  trigger?: string
  compact_summary?: string
  prompt?: string
}
