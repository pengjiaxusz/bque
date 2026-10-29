
# Architecture Index Generation Guide (`docs/architecture/README.md`)

> **Purpose**: When `docs/architecture/README.md` does not exist in the project, follow this guide with user approval to scan existing architecture documents and module structures, generating a standard, universal architecture index file.

---

## 1. Generation Principles

1. **Sole Authoritative Entry Point**: All architecture indexes recognize only `docs/architecture/README.md` without introducing redundant aliases or scattered entry points.
2. **Progressive Disclosure**: Keep the index file itself within 100-200 lines, providing fast semantic routing and module mapping without dumping raw implementation details.
3. **Semantic Hierarchy & Authority**:
   - Mark [Authoritative Specification] (mandatory lifecycles, protocols, core data flows, configuration schemas, etc.).
   - Mark [Design & Implementation] (detailed architecture and implementations for specific business modules/subsystems).
   - Attach [Associated Drafts / Research] (non-authoritative references and exploratory investigations).

---

## 2. Scanning & Extraction Steps

1. **Scan Architecture Directory**:
   - Check all `.md` files and subdirectories under `docs/architecture/`.
   - Check for auxiliary design directories like `docs/design/`.
2. **Extract Metadata**:
   For each architecture document, extract:
   - **Document Path**: Markdown link relative to `docs/architecture/` (e.g. `[system-architecture.md](./system-architecture.md)`).
   - **Core Responsibilities/Content**: Concise 1-2 sentence summary of what problem this architecture solves.
   - **Involved Modules / Code Paths**: Corresponding directories or packages in codebase (e.g. `src/core/`, `packages/server/`).
   - **Authority Level**: [Authoritative Specification] or [Design & Implementation].
3. **Output Standard Template**:
   Generate `docs/architecture/README.md` following this structure:

```markdown
# Project Architecture & Core Specification Index

> **AI / Developer Guide**: This file is the **sole authoritative architecture index** for this project.
> Before starting new features, cross-module changes, or complex debugging, consult the relevant documents for your domain.
> Items marked [Authoritative Specification] must be strictly followed; [Design & Implementation] guides concrete code.

---

## 1. Global System Architecture & Core Skeleton

| Authoritative Architecture Doc | Core Content / Scope | Related Modules / Code Paths | Authority Level |
| :--- | :--- | :--- | :--- |
| [`system-architecture.md`](./system-architecture.md) | Overall layered architecture, core process topology & lifecycle | `src/core/`, `src/bootstrap/` | [Authoritative Specification] |
...

## 2. Core Subsystems & Domain Modules

| Authoritative Architecture Doc | Core Content / Scope | Related Modules / Code Paths | Authority Level |
| :--- | :--- | :--- | :--- |
| [`communication-channel.md`](./communication-channel.md) | Cross-service/process communication protocol & RPC channels | `src/ipc/`, `src/transport/` | [Authoritative Specification] |
...

## 3. Associated Design Drafts & Explorations (Non-authoritative, reference only)

- `docs/design/xxx/` - Experimental feature design draft...
```

4. **Complete Generation**:
   Write to `docs/architecture/README.md` and summarize the indexed sections to the user.
