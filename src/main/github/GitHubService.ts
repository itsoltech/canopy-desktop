import { ok, err, okAsync, type ResultAsync } from 'neverthrow'
import type { PreferencesStore } from '../db/PreferencesStore'
import type { TaskTrackerConnection } from '../taskTracker/types'
import type { TaskTrackerManager } from '../taskTracker/TaskTrackerManager'
import { GitRepository } from '../git/GitRepository'
import { graphqlFetch } from './graphql'
import { parseGitHubRemote } from './remoteUrl'
import type { GitHubError } from './errors'
import type { RepoIdentifier, BranchPRMap, GitHubPR, CreatePRInput, GitHubRepoInfo } from './types'

const PR_SEARCH_QUERY = `
query($q: String!) {
  search(query: $q, type: ISSUE, first: 50) {
    nodes {
      ... on PullRequest {
        number
        title
        state
        url
        headRefName
        baseRefName
        isDraft
        reviewDecision
        commits(last: 1) {
          nodes {
            commit {
              statusCheckRollup { state }
            }
          }
        }
      }
    }
  }
}
`

const CREATE_PR_MUTATION = `
mutation($input: CreatePullRequestInput!) {
  createPullRequest(input: $input) {
    pullRequest {
      number
      title
      url
      headRefName
      baseRefName
      isDraft
    }
  }
}
`

const REPO_INFO_QUERY = `
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    id
    defaultBranchRef { name }
  }
}
`

interface SearchResponse {
  search: {
    nodes: Array<{
      number?: number
      title?: string
      state?: string
      url?: string
      headRefName?: string
      baseRefName?: string
      isDraft?: boolean
      reviewDecision?: string | null
      commits?: { nodes: Array<{ commit: { statusCheckRollup: { state: string } | null } }> }
    }>
  }
}

interface CreatePRResponse {
  createPullRequest: {
    pullRequest: {
      number: number
      title: string
      url: string
      headRefName: string
      baseRefName: string
      isDraft: boolean
    }
  }
}

interface RepoInfoResponse {
  repository: {
    id: string
    defaultBranchRef: { name: string } | null
  }
}

export class GitHubService {
  constructor(
    private preferencesStore: PreferencesStore,
    private taskTrackerManager: TaskTrackerManager,
  ) {}

  getRepoIdentifier(repoRoot: string): ResultAsync<RepoIdentifier, GitHubError> {
    return GitRepository.getRemoteUrl(repoRoot)
      .mapErr((): GitHubError => ({ _tag: 'NoGitHubRemote', repoRoot }))
      .andThen((url) => {
        const result = parseGitHubRemote(url)
        if (result.isErr()) return err(result.error)
        return ok(result.value)
      })
  }

  findGitHubConnection(
    repoRoot: string,
  ): ResultAsync<
    { connection: TaskTrackerConnection; token: string; repo: RepoIdentifier } | null,
    GitHubError
  > {
    return this.getRepoIdentifier(repoRoot).map((repo) => {
      const connections = this.taskTrackerManager.getConnections()
      // Match by exact projectKey, or empty projectKey on same host (or github.com default)
      const match = connections.find((c) => {
        if (c.provider !== 'github') return false
        if (c.projectKey) return c.projectKey === `${repo.owner}/${repo.repo}`
        const connHost = c.baseUrl ? new URL(c.baseUrl).hostname : 'github.com'
        return connHost === repo.host
      })
      if (!match) return null

      const token = this.preferencesStore.get(match.authPrefKey)
      if (!token) return null

      return { connection: match, token, repo }
    })
  }

  fetchOpenPRsForBranches(
    apiUrl: string,
    token: string,
    owner: string,
    repo: string,
    branches: string[],
  ): ResultAsync<BranchPRMap, GitHubError> {
    if (branches.length === 0) {
      return okAsync({} as BranchPRMap)
    }

    const headFilters = branches.map((b) => `head:${b}`).join(' ')
    const q = `repo:${owner}/${repo} is:pr is:open ${headFilters}`

    return graphqlFetch<SearchResponse>(apiUrl, token, PR_SEARCH_QUERY, { q }).map((data) => {
      const map: BranchPRMap = {}
      for (const node of data.search.nodes) {
        if (!node.headRefName || !node.number) continue
        const checksNode = node.commits?.nodes?.[0]?.commit?.statusCheckRollup
        map[node.headRefName] = {
          number: node.number,
          title: node.title ?? '',
          state: node.state ?? 'OPEN',
          url: node.url ?? '',
          headRefName: node.headRefName,
          baseRefName: node.baseRefName ?? '',
          isDraft: node.isDraft ?? false,
          reviewDecision: node.reviewDecision ?? null,
          checksState: checksNode?.state ?? null,
        }
      }
      return map
    })
  }

  createPR(
    apiUrl: string,
    token: string,
    input: CreatePRInput,
  ): ResultAsync<GitHubPR, GitHubError> {
    return graphqlFetch<CreatePRResponse>(apiUrl, token, CREATE_PR_MUTATION, {
      input,
    }).map((data) => {
      const pr = data.createPullRequest.pullRequest
      return {
        number: pr.number,
        title: pr.title,
        state: 'OPEN',
        url: pr.url,
        headRefName: pr.headRefName,
        baseRefName: pr.baseRefName,
        isDraft: pr.isDraft,
        reviewDecision: null,
        checksState: null,
      }
    })
  }

  getRepoInfo(
    apiUrl: string,
    token: string,
    owner: string,
    repo: string,
  ): ResultAsync<GitHubRepoInfo, GitHubError> {
    return graphqlFetch<RepoInfoResponse>(apiUrl, token, REPO_INFO_QUERY, {
      owner,
      name: repo,
    }).map((data) => ({
      id: data.repository.id,
      defaultBranch: data.repository.defaultBranchRef?.name ?? 'main',
    }))
  }
}
