
# Export Format & Front Matter Contract

Pull writes this layout; push accepts the same per-issue files. One format, both directions.

## 1. Bundle Layout

```
<out>/
├── README.md                     # index table + topology tree (templates/index.md + templates/topology.md)
├── issues/
│   └── NNNN-<kind>-<slug>.md     # one per issue (templates/issue.md)
└── data/
    ├── issues.json               # raw gh issue list payload
    └── relations.json            # per issue: parent / sub_issues / blocked_by / blocking / comments
```

`-Zip` compresses the same tree with the output directory name as the archive root.

## 2. Front Matter

Scalars and inline arrays only — no nested maps, no block sequences. Push parses exactly this subset.

| Field | Required on push | Meaning |
| :--- | :--- | :--- |
| `key` | yes | Local identity used by `parent` / `blocked_by`. Pull writes the issue number; hand-written plans may use any stable string (`S1-01`, `T-spec`). |
| `title` | yes | Issue title. |
| `labels` | no | `[a, b]`. Labels must already exist on the repo or push fails. |
| `state` | no | `open` / `closed`; informational on pull, ignored by push. |
| `kind` | no | `epic` / `ticket` / `issue`, or a name from `-KindRules`. Informational. |
| `parent` | no | Key of the parent issue. Becomes a sub-issue link. |
| `blocked_by` | no | `[key, key]`. Becomes native `blocked_by` edges. |
| `blocks` | no | Inverse view, computed on pull. Ignored by push. |
| `number` | no | Real issue number, filled by push and by pull. |
| `url` | no | Canonical URL. |
| `created_at` / `updated_at` | no | ISO timestamps. |

Unknown keys are preserved verbatim in the file and ignored by push.

## 3. Kind Resolution

Order of precedence:

1. `-KindRules "spec=^Spec\s,ticket=^S\d-\d\d"` — first matching regex on the title wins.
2. No rule matches: `epic` if the issue has sub-issues, else `ticket` if it has a parent, else `issue`.

Kind only affects the file name and the index column; nothing in the API depends on it.

## 4. Templates

`templates/` holds three plain Markdown files with `{{PLACEHOLDER}}` slots. Copy any of them next to your bundle and pass `-Templates <dir>` to override the built-in set; no code changes.

| Template | Placeholders |
| :--- | :--- |
| `issue.md` | `{{FRONT_MATTER}}` `{{TITLE}}` `{{META_TABLE}}` `{{BODY}}` `{{COMMENTS}}` |
| `index.md` | `{{REPO}}` `{{COUNT}}` `{{RANGE}}` `{{GENERATED_AT}}` `{{TABLE}}` `{{TOPOLOGY}}` `{{NOTES}}` |
| `topology.md` | `{{TREE}}` `{{NOTE}}` |

Missing placeholders are left empty rather than failing the render.

## 5. Resume State

Push writes `<mapping>.state.json` next to the mapping file:

```json
{ "pending_creates": ["S3-02"], "pending_parents": ["S3-02"], "pending_edges": [["S3-04", "S3-01"]] }
```

`-Resume` reads it and continues. `-DryRun` never writes state.
