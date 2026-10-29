---
name: write-a-skill
description: >-

  Create new Agent skills with correct structure, progressive disclosure, and bundled resources. Use when the user wants to create, write, or build a new skill.
  Triggers: write-a-skill, new skill, create skill, skill template.

---

# Write a Skill

Create a new Agent skill that follows the repository's skill conventions (auto-discovery, bilingual, managed block + project-specific area).

## Workflow

### 1. Gather requirements

Ask the user:

- What task/domain does the skill cover?
- Which concrete use cases must it handle?
- Does it need executable scripts, or just instructions?
- Does it need bundled reference material?

### 2. Draft the skill

Create:

- `SKILL.md` with concise instructions
- Additional reference files if the body exceeds ~500 lines
- Utility scripts if deterministic operations are needed

### 3. Review with the user

Show the draft and ask:

- Does it cover your use cases?
- Anything missing or unclear?
- Any section that should be more detailed or more concise?

## Skill Structure

```
skill-name/
├── SKILL.md           # Main instructions (required)
├── REFERENCE.md       # Detailed docs (as needed)
├── EXAMPLES.md        # Usage examples (as needed)
└── scripts/           # Helper scripts (as needed)
    └── helper.js
```

In this repo a skill lives at `templates/agent/.agents/skills/<name>/SKILL.md` (auto-discovered by `Templates::list_skills()`, no registration). It is rendered to the generated project at `.agents/skills/<name>/SKILL.md` and filtered by `options["skills"]` (`skill_name_of` / `selected_skills` in `crates/core/src/engine.rs`; missing option means include all).

## SKILL.md Template

```md
---
name: skill-name
description: Brief capability description. Use when [specific trigger condition].
---

# Skill Name

## Quick Start

[Minimal runnable example]

## Workflow

[Step-by-step flow with checklists for complex tasks]

## Advanced

[Link to separate file: See [REFERENCE.md](REFERENCE.md)]
```

## Description Requirements

The description is the **only content the Agent sees when choosing which skill to load**. It is shown together with all other installed skills in the system prompt.

Goal — tell the Agent:

1. What capability this skill provides
2. When/why to trigger it (concrete keywords, contexts, file types)

Format:

- Max 1024 characters
- Written in third person
- Sentence 1: what it does
- Sentence 2: "Use when [specific trigger condition]"

Good:

```
Extract text and tables from PDF files, fill forms, and merge documents. Use when handling PDF files or when the user mentions PDF, forms, or document extraction.
```

Bad:

```
Help with documents.
```

## When to Add Scripts

Add utility scripts when:

- The operation is deterministic (validation, formatting)
- The same code would be generated repeatedly
- Errors need explicit handling

Scripts save tokens and improve reliability compared to generated code.

## When to Split Files

Split into separate files when:

- `SKILL.md` exceeds ~100 lines
- Content spans different domains (e.g. finance vs sales modes)
- Advanced features are rarely needed

Keep `SKILL.md` lean; move infrequent detail to `REFERENCE.md` / `EXAMPLES.md` and link to it (one level deep only).

## Review Checklist

After drafting, verify:

- [ ] Description contains trigger condition ("Use when ...")
- [ ] `SKILL.md` is under ~100 lines (or split)
- [ ] No time-sensitive information
- [ ] Consistent terminology
- [ ] Concrete examples included
- [ ] References are at most one level deep
- [ ] Bilingual requirements met (see below)

## Bilingual & Framework Requirements (this repo)

- All agent-layer content (this `SKILL.md` + `AGENTS.md`) must be bilingual via `{% if options["skill_lang"] == "en" %}` ... `{% else %}` ... `{% endif %}` branches (zh is default); frontmatter `description` must also branch.
- Copy the structure of the `commit` skill for new skills; when adding a language, extend the `skill_lang` validation in CLI / GUI / core.
- `SKILL.md` = managed framework block (`PENGJ_TEMPLATE_START/END`, replaced in place on `update`) + project-specific area **outside** the block (never overwritten). Document-style customization and commit-gate definitions live outside the block; executable gate form is project-defined.

