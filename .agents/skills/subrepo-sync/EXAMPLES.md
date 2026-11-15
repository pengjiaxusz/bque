
# Sub-Repository Upgrade & Host Code Adaptation Examples (EXAMPLES.md)

This document presents practical real-world scenarios for sub-repository upgrades, illustrating the full workflow from commit diff extraction and semantic impact clustering to host code adaptation and refactoring.

---

## Example 1: Detecting Unapplied Commits & Multi-Dimensional Impact Clustering

Before upgrading a sub-repository, run the detection script to generate an impact profile:

```powershell
pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "submodules/shared-ui"
```

**Console Output Example**:
```text
=================================================================
             shared-ui Subrepo Unapplied Commits Analysis        
=================================================================
[Current Old Pin]: a1b2c3d (a1b2c3d4e5f6...)
                   └─ fix(dropdown): prevent click propagation outside modal
[Target New Pin]:  7e8f9a0 (7e8f9a012345...) [source: ..\shared-ui -> HEAD]
                   └─ feat(code-block): support theme-aware syntax highlighting
-----------------------------------------------------------------
Unapplied commits found: 18

[Subsystems & Clustered Impact]
  💥 Breaking Changes [1 commit] - Needs critical review:
     - 4f2a1b9 feat(button)!: rename variant 'danger' to 'destructive'

  🎬 Motion & Transition System (Motion) [3 commits]:
     - b9c8d7e feat(motion): add ease-standard and ease-decelerate tokens
     - 3a4b5c6 fix(dialog): resolve exit transition flickers

  🧩 Standard Widgets & Component API Changes (Components) [5 commits]:
     - 7e8f9a0 feat(code-block): support theme-aware syntax highlighting
     - 1a2b3c4 feat(duration-input): add duration input widget with unit selector

-----------------------------------------------------------------
[Full Unapplied Commits List (git log oneline)]:
  7e8f9a0 feat(code-block): support theme-aware syntax highlighting
  4f2a1b9 feat(button)!: rename variant 'danger' to 'destructive'
  ...
=================================================================

💡 Hint: To check out and update this subrepo to the target pin directly, run:
  pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath 'submodules/shared-ui' -Update
```

---

## Example 2: Refactoring Host Duplications into Upstream Standard Widgets

**Scenario**:
The clustering report reveals that upstream introduced a new `<DurationInput>` standard component. The host codebase currently has 200+ lines of handwritten unit conversion and input handling:

### Before Refactoring (Host Handwritten Wheel)
```tsx
// Temporary host business implementation: verbose and hard to maintain
export function TimeDurationEditor({ valueMs, onChange }: Props) {
  const [unit, setUnit] = useState('ms');
  const [val, setVal] = useState(valueMs);

  const handleUnitChange = (u: string) => {
    // manual conversion math...
  };

  return (
    <div className="flex gap-1">
      <input type="number" value={val} onChange={...} />
      <select value={unit} onChange={...}>...</select>
    </div>
  );
}
```

### After Refactoring (Adopt Upstream Standard Widget)
```tsx
import { DurationInput } from "@org/shared-ui";

export function TimeDurationEditor({ valueMs, onChange }: Props) {
  return (
    <DurationInput
      valueMicroseconds={valueMs * 1000}
      onChangeMicroseconds={(us) => onChange(us ? us / 1000 : 0)}
      preset="standard"
    />
  );
}
```

**Results**:
- Deleted 200+ lines of duplicate boilerplate;
- Unified product-grade interaction feel and keyboard accessibility;
- Enforced the "Component Bootstrap Law" in the host architecture.

---

## Example 3: CMake / FetchContent Dependency Upgrade & Refactoring

For dependencies managed via CMake FetchContent or local cache directories (e.g. `target/debug/_deps/mylib-src`):

```powershell
# 1. Inspect unapplied commits
pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "target/debug/_deps/mylib-src"

# 2. Perform update and reconfigure CMake
pwsh .agents/skills/subrepo-sync/scripts/show-unapplied-commits.ps1 -SubrepoPath "target/debug/_deps/mylib-src" -Update
cmake -B target/debug -S .

# 3. Enforce host test gates
ctest --test-dir target/debug --output-on-failure
```

---

## Example 4: Commit Specification after Upgrade

After adaptation passes local verification gates, create a conventional commit:

```bash
git add submodules/shared-ui src/components/TimeDurationEditor.tsx
git commit -m "chore(deps): update shared-ui to 7e8f9a0 and adopt standard DurationInput widget"
git push
```
