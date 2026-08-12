export type CiJobPickerProvider = 'teamcity' | 'github-actions'

export function ciJobPickerCopy(provider: CiJobPickerProvider): {
  empty: string
  description: string
} {
  if (provider === 'github-actions') {
    return {
      empty: 'The repository exposes no dispatchable workflows.',
      description:
        'These are the dispatchable workflows the GitHub repository exposes. Select the ones that belong in Canopy (the CI/CD section, Run workflow and the branch context menu).',
    }
  }
  return {
    empty: 'The server exposes no jobs.',
    description:
      'These are all the jobs (build configurations) the TeamCity server exposes. Select the ones that belong to this repository (the CI/CD section, Run job and the branch context menu).',
  }
}
