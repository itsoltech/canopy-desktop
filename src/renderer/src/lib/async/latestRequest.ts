export interface LatestRequestToken {
  sequence: number
  scope: string
}

export function createLatestRequestGuard(): {
  begin: (scope: string) => LatestRequestToken
  invalidate: () => void
  isLatest: (token: LatestRequestToken) => boolean
  isCurrent: (token: LatestRequestToken, scope: string) => boolean
} {
  let sequence = 0

  return {
    begin(scope) {
      return { sequence: ++sequence, scope }
    },
    invalidate() {
      sequence += 1
    },
    isLatest(token) {
      return token.sequence === sequence
    },
    isCurrent(token, scope) {
      return token.sequence === sequence && token.scope === scope
    },
  }
}
