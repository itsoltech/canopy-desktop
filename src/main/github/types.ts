export interface RepoIdentifier {
  owner: string
  repo: string
  host: string
  apiUrl: string
}

export interface GitHubPR {
  number: number
  title: string
  state: string
  url: string
  headRefName: string
  baseRefName: string
  isDraft: boolean
  reviewDecision: string | null
  checksState: string | null
}

export type BranchPRMap = Record<string, GitHubPR>

export interface CreatePRInput {
  repositoryId: string
  headRefName: string
  baseRefName: string
  title: string
  body: string
  draft: boolean
}

export interface GitHubUser {
  login: string
  name: string | null
}

export interface GitHubRepoInfo {
  id: string
  defaultBranch: string
}

export interface GitHubLabel {
  name: string
  color: string
}

export interface GitHubMilestone {
  number: number
  title: string
  state: string
}
