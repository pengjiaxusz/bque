
# Domain Architecture Doc Generation Guide (`docs/architecture/<module>.md`)

> **Purpose**: When an authoritative architecture document is missing for a core domain or subsystem, generate a standard, universal architecture document following this guide and software engineering best practices after user consent.

---

## 1. Universal Architecture Principles & Requirements

1. **Clear Responsibilities & Boundaries**:
   - Explicitly define the role and position of this module within the overall system.
   - Clearly delineate input and output contracts; avoid handling responsibilities outside this module's scope.
2. **Standardization & Generality**:
   - Follow a standardized architecture layout without binding to ad-hoc project conventions.
   - Use precise technical terminology and domain concepts.
3. **Appropriate Length & Progressive Structure**:
   - Keep individual documents within 200-400 lines.
   - Must contain the 4 core sections: System Boundaries, Core Data Flow, Source Mapping, and Invariants & Anti-Patterns.

---

## 2. Standard Domain Architecture Doc Template

The generated architecture document should contain the following standard sections:

```markdown
# <Module / Subsystem Name> Architecture Specification

## 1. Role & System Boundaries
- **Core Role**: One-sentence summary of the core responsibility and primary problem solved.
- **Scope Delineation**:
  - **In-Scope (Handled by this module)**: ...
  - **Out-of-Scope (Delegated externally)**: ...

## 2. Core Data Flow & Lifecycle
- **Core Data Flow / Call Sequence**: Complete data flow from request trigger to final processing.
- **Concurrency & Context Model**: Thread/coroutine/runtime execution context, state sharing, and synchronization mechanisms.
- **Lifecycle**: Initialization, hot-reload / runtime maintenance, and shutdown/cleanup sequence.

## 3. Code Module Mapping Table
| Logical Role / Layer | Corresponding File / Directory Path | Responsibilities & Key Interfaces / Types |
| :--- | :--- | :--- |
| Entry & Facade | `src/xxx/entry.*` | Public API surface & parameter validation |
| Core Logic & State | `src/xxx/core.*` | Domain state management & core transformations |
| External Adapter / Driver | `src/xxx/adapter.*` | Low-level calls & external system interaction |

## 4. Invariants & Forbidden Anti-Patterns
- **Invariant 1**: Non-negotiable constraint (e.g. no cross-layer direct access, unidirectional read-only state).
- **Invariant 2**: Non-functional constraint (memory limits, latency budgets, performance requirements).
- **Common Anti-Patterns**: Pitfalls and forbidden practices to strictly avoid during development.
```

---

## 3. Self-Registration Feedback Loop

After generating the new domain architecture doc, **always register the new doc entry into the `docs/architecture/README.md` index table** to ensure subsequent agents and developers can discover it immediately via the main index.
