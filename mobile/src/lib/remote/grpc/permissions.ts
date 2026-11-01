import { Scope } from './schema'

export function scopeLabel(scope: Scope): string {
  switch (scope) {
    case Scope.SCOPE_UNSPECIFIED: return 'Unspecified'
    case Scope.SCOPE_READ: return 'Read'
    case Scope.SCOPE_CHAT: return 'Chat'
    case Scope.SCOPE_APPROVE: return 'Approve'
    case Scope.SCOPE_FILES_GIT: return 'Files/Git'
    case Scope.SCOPE_TERMINAL: return 'Terminal'
    case Scope.SCOPE_TASKS: return 'Tasks'
    case Scope.SCOPE_ADMIN: return 'Admin'
    default: return `Unknown scope (${scope})`
  }
}
