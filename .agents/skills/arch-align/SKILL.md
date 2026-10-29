---
name: arch-align
description: >-

  Universal architecture search and constraint alignment skill before feature development and complex debugging. Discovers the authoritative index at docs/architecture/README.md via progressive disclosure; asks the user and generates missing indexes/docs following guidelines; aligns architectural invariants and boundaries before coding. Use when inspecting architecture, planning features, modifying cross-module code, or when the user mentions "check architecture", "architecture docs", or "arch-align". Triggers: arch-align, architecture-alignment, check architecture, architecture docs.

---


# Architecture Alignment (arch-align)

## Quick Start

Before starting feature design, refactoring, or diagnosing complex cross-module issues, follow this workflow to align on architecture:

```
[Check docs/architecture/README.md] ──► [Inspect domain architecture docs] ──► [Extract invariants & boundaries] ──► [Proceed to implementation]
```

## Workflow

### Step 1: Locate authoritative architecture index

1. Check if `docs/architecture/README.md` exists (**the sole authoritative architecture index; do not look for alternative file names**).
2. **If the index file does NOT exist**:
   - Ask the user: "Detected that `docs/architecture/README.md` does not exist. Would you like me to scan existing architecture docs and codebase structure to generate the index file?"
   - If user agrees, read reference guide `references/GENERATE-INDEX.md` to generate the index, then proceed.
   - If user declines, fall back to current context and general conventions.
3. **If the index file exists**:
   - Read `docs/architecture/README.md` to understand subsystem and domain architecture distribution across the project.

### Step 2: Identify domain docs & progressive inspection

1. Search the main index for the most relevant authoritative architecture doc based on modules/directories/keywords involved in current task.
2. **If relevant architecture doc is found**:
   - Accurately read 1-2 corresponding docs.
3. **If no authoritative architecture doc is indexed for this domain**:
   - Check if the project has relevant design drafts or research notes (e.g. `docs/design/` or `docs/设计/`).
   - Ask the user: "Authoritative architecture documentation is missing for this domain. Would you like me to generate a standard architecture doc from existing code implementation and drafts?"
   - If user agrees, read reference guide `references/GENERATE-DOC.md` to generate the new document, and **register it into the `docs/architecture/README.md` index table**.

### Step 3: Extract invariants & enter development

After reading the relevant architecture docs, briefly summarize to the user:
1. **Reference Docs**: Paths of the aligned architecture documents.
2. **Core Invariants & Boundaries**: Mandatory architectural constraints for this task (e.g., data flow direction, module boundaries, concurrency and lifecycle rules, forbidden anti-patterns).
3. **Implementation Plan**: Proposed coding or planning steps aligned with above constraints.

## Detailed Guides (Progressive Disclosure)

- [Architecture Index Generation Guide](references/GENERATE-INDEX.md)
- [Domain Architecture Doc Generation Guide](references/GENERATE-DOC.md)
