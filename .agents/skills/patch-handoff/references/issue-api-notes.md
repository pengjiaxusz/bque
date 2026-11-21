
# GitHub Issues API Notes

Everything here was verified against `gh` 2.94.0. Do not replace it with folklore.

## 1. Reading: `gh issue list --json` is enough

Available fields (identical set for `gh issue view`):

```
assignees author blockedBy blocking body closed closedAt closedByPullRequestsReferences
comments createdAt id isPinned issueType labels milestone number parent projectCards
projectItems reactionGroups state stateReason subIssues subIssuesSummary title updatedAt url
```

`parent`, `subIssues`, `blockedBy`, `blocking`, and `subIssuesSummary` cover every relation this skill needs. One paginated call beats N+1 calls.

### Shape trap: connections, not arrays

`subIssues`, `blockedBy`, and `blocking` are GraphQL connection objects:

```json
{ "blockedBy": { "nodes": [ { "number": 26, "title": "...", "state": "OPEN" } ] } }
```

`parent` is either a single issue object or `null`. On a list result, `.blockedBy | map(.number)` fails with `expected an object but got: array` — read `.blockedBy.nodes[]` instead. Always null-check `parent` before touching `.number`.

### Comments

`--json comments` on `list` is not guaranteed to carry complete bodies. When full comment text matters, fetch `/repos/{owner}/{repo}/issues/{number}/comments?per_page=100` per issue.

## 2. Writing: two REST endpoints only

`gh issue` exposes create / list / view / close / comment / edit / develop / lock / pin / transfer / reopen / delete. It has **no** sub-issue and **no** dependency subcommand, so these two go through `gh api`:

```powershell
# parent-child
gh api --method POST "repos/$Repo/issues/$ParentNumber/sub_issues" -F "sub_issue_id=$ChildDbId"

# dependency (child is blocked by blocker)
gh api --method POST "repos/$Repo/issues/$ChildNumber/dependencies/blocked_by" -F "issue_id=$BlockerDbId"
```

Both payloads take the **database id**, not the issue number:

```powershell
gh api "repos/$Repo/issues/31" --jq ".id"
```

Read the current state before writing (`GET .../sub_issues`, `GET .../dependencies/blocked_by`) so a re-run adds only what is missing.

## 3. Operational constraints

- **Wall clock**: long upload runs get killed near 120s in hosted agent shells. Budget work per invocation (default 110s), persist progress, and resume rather than restart.
- **Transport flake**: `net/http: TLS handshake timeout` and `graphql: EOF` occur intermittently on healthy networks. Retry each call a few times with a short backoff before failing.
- **Rate limits**: creation is cheap, but each read-before-write costs a call. Cache database ids within a run.
- **Ordering**: create every issue before wiring edges. A blocked_by edge requires both endpoints to exist.
