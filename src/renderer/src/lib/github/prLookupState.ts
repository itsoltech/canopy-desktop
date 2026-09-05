export interface PRLookupState {
  loading: boolean
  error: string
}

export function pendingPRLookup(): PRLookupState {
  return { loading: true, error: '' }
}

export function settledPRLookup(error = ''): PRLookupState {
  return { loading: false, error }
}
