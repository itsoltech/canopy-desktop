---
description: Fix PR review feedback from inline threads, top-level PR comments, and review summaries
argument-hint: '[PR number]'
allowed-tools: ['Bash', 'Read', 'Edit', 'Write']
disable-model-invocation: true
---

# Fix PR review comments

Address PR review feedback from inline threads, top-level PR comments, and review summaries:
fix code, reply to each comment, resolve inline threads, commit.

**PR number (optional):** `$ARGUMENTS`

## Step 1: Identify the PR, repo, and ensure correct branch

Get repo owner and name:

```bash
gh repo view --json owner,name -q '"\(.owner.login)/\(.name)"'
```

Split into OWNER and REPO variables for GraphQL queries.

**If `$ARGUMENTS` is empty** — use the PR associated with the current branch:

```bash
gh pr view --json number,url -q '.number'
```

**If `$ARGUMENTS` contains a PR number** — fetch that PR's head branch and switch to it if not already on it:

```bash
PR_BRANCH=$(gh pr view $ARGUMENTS --json headRefName -q '.headRefName')
CURRENT_BRANCH=$(git branch --show-current)
```

If `PR_BRANCH != CURRENT_BRANCH`, checkout the PR branch:

```bash
git checkout "$PR_BRANCH"
```

If the branch doesn't exist locally yet:

```bash
gh pr checkout $ARGUMENTS
```

## Step 2: Fetch review feedback

Fetch both inline review threads and top-level PR conversation comments. Claude/other review
bots may leave actionable feedback in either place.

Single GraphQL query (substitute OWNER, REPO, PR_NUMBER):

```bash
gh api graphql -f query='
query {
  repository(owner: "OWNER", name: "REPO") {
    pullRequest(number: PR_NUMBER) {
      comments(last: 100) {
        nodes {
          id
          databaseId
          body
          author { login }
          createdAt
          url
        }
      }
      reviews(last: 50) {
        nodes {
          id
          databaseId
          body
          author { login }
          state
          submittedAt
          url
        }
      }
      reviewThreads(last: 100) {
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          startLine
          comments(first: 10) {
            nodes {
              id
              databaseId
              body
              author { login }
              path
              line
              startLine
              originalLine
              diffHunk
            }
          }
        }
      }
    }
  }
}'
```

Filters:

- Inline threads: keep only threads where `isResolved == false`. Skip `isOutdated == true`
  threads (code moved, comment no longer applies).
- Top-level PR comments: read all recent `comments.nodes`. Treat comments from review bots
  (for example `claude[bot]`) as potentially actionable even when there are no inline threads.
  Ignore comments that are only status notifications, CI output, or already addressed by later
  comments/commits.
- Review summaries: read non-empty `reviews.nodes[].body` as potentially actionable global
  feedback. This catches review summary bodies that are not attached to a line.

If zero unresolved inline threads and zero actionable top-level/review-summary comments remain,
report "No actionable PR review feedback" and stop.

## Step 3: Fix each feedback item

For each unresolved, non-outdated thread:

1. Read the first comment's `body` to understand what the reviewer wants.
2. Read subsequent comments in the thread for follow-up context.
3. Note `path` and `line` (and `startLine` if multi-line).
4. Read the file at the relevant lines using the Read tool with enough surrounding context.
5. Determine the fix. If the request is ambiguous or requires a design decision, **skip it** and add to the "needs manual attention" list.
6. Apply the fix using the Edit tool.
7. Compose a short, specific reply (1-2 sentences) describing what was changed. Example: "Added sender validation guard matching the pattern from `setMouseIgnore`." Do NOT write generic replies like "Fixed as suggested."

For each actionable top-level PR comment or review summary:

1. Read the full `body`, including bullet lists and code blocks. These comments often summarize
   multiple issues.
2. Split the body into separate actionable items. Ignore praise, summaries without requested
   changes, and stale items contradicted by newer comments.
3. Locate relevant files with `rg` and read enough context before editing.
4. If an item is ambiguous or architectural, skip it and add it to "needs manual attention".
5. Apply fixes for clear items.
6. Compose one concise top-level reply for the original comment/review summary listing what was
   addressed and what was skipped, if anything.

## Step 4: Reply and resolve each thread

For each fixed inline thread, run both mutations. Use the PR number and `databaseId` of the first
comment in the thread for REST reply, or use GraphQL:

**Reply via GraphQL:**

```bash
gh api graphql -f query='
mutation {
  addPullRequestReviewThreadReply(input: {
    pullRequestReviewThreadId: "THREAD_ID",
    body: "REPLY_TEXT"
  }) {
    comment { id }
  }
}'
```

**Resolve the thread:**

```bash
gh api graphql -f query='
mutation {
  resolveReviewThread(input: {
    threadId: "THREAD_ID"
  }) {
    thread { isResolved }
  }
}'
```

For praise/acknowledgment comments (no code change needed), reply with a brief "Thanks!" and resolve.

For fixed top-level PR comments or review summaries, do not call `resolveReviewThread` because
there is no thread to resolve. Add a normal PR conversation reply instead:

```bash
gh pr comment PR_NUMBER --body "REPLY_TEXT"
```

Mention the original comment author or quote a short identifying phrase if several top-level
comments are being addressed.

**Batch optimization:** if multiple threads can be resolved at once, combine mutations:

```bash
gh api graphql -f query='
mutation {
  a: resolveReviewThread(input: {threadId: "ID1"}) { thread { isResolved } }
  b: resolveReviewThread(input: {threadId: "ID2"}) { thread { isResolved } }
}'
```

## Step 5: Commit and report

Stage only the files modified during this process. Create a single commit:

```
fix: address PR review feedback
```

Include a body listing each file and what was changed.

Print summary:

- Threads resolved (count and brief descriptions)
- Top-level PR comments/review summaries addressed (count and brief descriptions)
- Threads skipped as needing manual attention (with reasons)
- Files modified
- Remind user to `git push` to update the PR

## Rules

- Do NOT resolve a thread unless you actually fixed the issue or it requires no code change (praise/ack).
- Do NOT ignore top-level PR comments or review summaries just because there are no inline threads.
- If a comment is ambiguous, architectural, or you disagree with the approach, skip it and list in report.
- Escape double quotes, newlines, and backslashes when injecting reply text into GraphQL queries.
- Use `rg` instead of `grep` and `fd` instead of `find`.
- Do NOT auto-push. Remind the user to push.
