/** Renderer-side view of a tracker task — the fields the pickers and create flows work with
 *  (a structural subset of the main process's TrackerTask, safe to snapshot over IPC). */
export interface TrackerTaskLite {
  key: string
  summary: string
  description: string
  status: string
  statusCategory?: string
  priority: string
  type: string
  parentKey?: string
  sprintName?: string
  sprintNumber?: number
  assignee?: string
  url?: string
}

export type TrackerProviderKind = 'jira' | 'youtrack' | 'github'
