// These types used to be a mobile-local mock mirror of the desktop snapshot
// types. They're now re-exports from the canonical remote protocol location
// so mock data stays structurally compatible with live data.
export type {
  ProjectSnapshot,
  TabSnapshot,
  WorktreeSnapshot,
} from '@/lib/remote/protocol/state-snapshot'
