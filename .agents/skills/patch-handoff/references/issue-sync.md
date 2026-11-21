# Issue Tooling — Clone the Backlog Out, Push a Bundle Back

The two PowerShell scripts bundled with this skill move issues between GitHub and a portable Markdown
bundle: one command each way, no third-party binary, no local mirror committed to the repo. The push
path is what a handoff package needs (see [issue-import.md](./issue-import.md)); the pull path exists
so an export can be read, diffed or handed to another agent without GitHub access.

Both ship inside this skill — there is no separate `issue-sync` skill to install, and no dependency
on a skill the project may not have generated.

## Quick Start

```powershell
# Pull: open issues -> ./issue-export/ (+ optional zip)
pwsh .agents/skills/patch-handoff/scripts/issue-pull.ps1 -Repo owner/name -Out ./issue-export -Zip

# Push: local issue files -> real issues, then wire parents and dependencies
pwsh .agents/skills/patch-handoff/scripts/issue-push.ps1 -Repo owner/name -InputDir ./issue-plan -DryRun
```

Always run `-DryRun` first on push. It prints every create/link without touching the remote.

## Read Path

`issue-pull.ps1` calls `gh issue list --json` once (plus `--label` / `--search` / `--state` / `--limit`
pass-through), normalizes the GraphQL connection shapes, then renders:

- `README.md` — index table + dependency topology tree
- `issues/NNNN-<kind>-<slug>.md` — front matter metadata, body, comments
- `data/issues.json`, `data/relations.json` — untouched API payloads

Layout is driven by the templates in `templates/` next to `scripts/`, so restyling an export never
touches code.

## Write Path

`issue-push.ps1` reads a directory of Markdown files whose front matter carries `key`, `title`,
`labels`, optional `parent` and `blocked_by` (keys, not numbers). It creates missing issues, links
parents as sub-issues, then links dependencies as native `blocked_by`. The output is a `mapping.json`
from local key to real number.

Three invariants it enforces:

1. **Idempotent** — existing edges are read before writing, so re-running only fills gaps.
2. **Chunked** — one invocation stays under a wall-clock budget (default 110s) and leaves a resumable
   state file; pass `-Resume` to continue.
3. **Retried** — every API call retries on transport errors.

## Boundaries

- Read and write both require `gh` (>= 2.94) and an authenticated account; nothing else.
- No two-way conflict merge, no `.issues/` mirror committed to the repo, no hand-written GraphQL.
- `gh issue` has no sub-issue or dependency subcommands — those two links go through `gh api` REST
  endpoints. See [issue-api-notes.md](./issue-api-notes.md).
- Output format and front matter fields: [issue-export-format.md](./issue-export-format.md).
