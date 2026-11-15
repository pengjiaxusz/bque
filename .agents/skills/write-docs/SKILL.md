---
name: write-docs
description: >-

  Write or update any project documentation: first find the rules the project already has, otherwise follow how existing docs are actually written, and only write freely when there is nothing to learn from. Use when creating or revising architecture docs, design notes, READMEs, guides, specs, ADRs, or any prose document, or when a change has left docs out of date.
  Triggers: write-docs, write docs, update docs, documentation, docs, doc, README, architecture doc, design doc, spec, ADR, guide, doc drift, stale docs.

---

<!-- PENGJ_TEMPLATE_START -->

# Write Docs

How a document should be written is decided by the project, not by this skill. This skill's job: find the rules the project already has and follow them; when no rule exists, follow how existing docs are actually written; only when there is no doc to learn from do you write freely.

## Stance

- **Rules belong to the project.** No template, no fixed section skeleton, no length cap, no tag system, no mandatory diagram.
- **Find rules before writing.** Two extra searches beat overwriting a project's conventions with your own taste.
- **Precedence:** rules stated by the target doc or its directory > project convention files > the actual shape of sibling docs > free-form.
- **Self-contained.** It assumes no upstream process or other skill exists. Look up whatever you need at the moment you need it.

## Steps

### 1. Pin down the target

- Which file, and whether it is new or an update. If the user gave a path, use it. If not, infer from the existing doc layout (where similar docs live, how they are named); ask one question only when the layout gives no hint.
- For an update, read the whole document first — know its current structure and voice before changing anything.

### 2. Find the rules (in this order; stop at the first hit)

1. A rules section inside the target doc or its directory README. Many projects put a "how to write these docs" section at the end of the directory README: layering, naming, forbidden content, diagram requirements.
2. Convention files: `AGENTS.md` / `CLAUDE.md` / `CONTRIBUTING.md` / the doc directory's `README.md`, plus tooling that encodes format (markdownlint, vale, dprint, prettier, doc templates, site config).
3. The actual shape of sibling docs: sample 2-3 recently updated ones and read their section order and depth, length, voice (prose vs telegraphic), language (including bilingual or mixed), whether and what they draw, link and reference style, file naming.

- When several rule sets exist, take them in the order above. When they conflict, follow the more specific one (same directory beats global) and state the conflict in one line when you deliver; do not silently rewrite other docs into agreement.

### 3. Write

- **Explicit rules exist:** follow them item by item — language, naming, length, structure, forbidden content. When a rule clashes with your taste, the rule wins.
- **Only sibling docs exist:** treat the shape you extracted in 2.3 as the rule. The new doc must read like the same people wrote it; do not introduce sections, diagram types, or formatting the existing docs never use.
- **Nothing at all:** decide from the reader and the purpose — who reads this, what they must be able to do afterwards, how much background they need. Requirements only: accurate, readable, no padding, nothing invented. Length follows content.

### 4. Check before delivering

- **Facts:** every claim traces back to code, config, command output, or explicit user words. Mark inferences as inferences, or verify first.
- **Links and paths:** referenced files exist; referenced commands and options are spelled correctly.
- **Consistency:** one concept keeps one term, the same as the rest of the docs; no self-contradiction inside the document.
- **Drift:** if the doc is already wrong about reality, fix only what this task touches and point out the remaining mismatches. No full rewrite unless asked.

## Gotchas

- **Mistaking taste for convention.** Assume no rule exists until you find evidence of one.
- **Update becoming rewrite.** Keep the original wording and structure wherever possible; minimize the diff.
- **Forcing a skeleton.** Do not bolt "Overview / Background / Summary" onto a doc that never had them just to fill space.
- **Unsourced numbers and limits.** Every figure, budget, or boundary carries its source or the conditions it was measured under.
- **New doc, no index entry.** If the project maintains an index or directory README, register the new document there — following the existing habit, not a new one.

<!-- PENGJ_TEMPLATE_END -->


<!-- The following is the project-specific area: template updates only replace the managed block above; this area is owned by the project and fully preserved. -->
## Project-Specific Doc Rules

> Pin down this project's doc layout, naming, audience, and any "never do this" rules here. If the rules already live somewhere else (for example a rules section at the end of the docs directory README), just point to it — do not copy it.
