---
name: write-just-reference
description: >-

  Detailed command-to-recipe mapping for justfile authoring: heavy command wrappers, Windows/Unix attribute pairs, variadic argument forwarding, and the legitimate no-bque downgrade. Loaded on demand from the write-just skill.

---


# just Recipe Command Map

## 1. Native command → wrapped recipe

Applies when `bque` is in the project's `layers`. On Windows the recipe takes the global lock and `Idle` priority; the `[unix]` branch passes through.

| Native command | Recipe | Windows body | Unix body |
| :--- | :--- | :--- | :--- |
| `cargo run` | `just run` | `bque run -- cargo run …` | `cargo run …` |
| `cargo build --release` | `just build` | `bque run -- cargo build …` | `cargo build …` |
| `cargo clippy --all-targets …` | `just check` | `bque run -- cargo clippy …` | `cargo clippy …` |
| `cargo test` | `just test` | `bque run -- cargo test …` | `cargo test …` |
| `cargo bench` | `just bench` | `bque bench -- cargo bench …` | `cargo bench …` |
| `cmake --build <dir>` | `just build` | `bque run -- cmake --build …` | `cmake --build …` |
| `ninja -C <dir> <target>` | `just test-build <target>` | `bque run -- ninja -C …` | `ninja -C …` |
| `ctest --output-on-failure` | `just test` | `bque run -- ctest …` | `ctest …` |
| `pnpm build` / `tsc` | `just build` | `bque run -- pnpm build` | `pnpm build` |
| `cargo fmt` | `just fmt` | direct (lightweight) | direct |
| ad-hoc heavy command | — (no wrapper) | `bque run -- <cmd>` | `<cmd>` |
| ad-hoc benchmark | — (no wrapper) | `bque bench -- <cmd>` | `<cmd>` |

Ad-hoc commands have no wrapper recipe: a `just` wrapper can forward the command but not bque's own
flags (`--cwd`, `--observer`, `--priority`), so the shell is the right place. Promote a recurring one
to a recipe of its own.

## 2. Attribute pairs

- `[windows]` / `[unix]` must appear together for any recipe whose body differs. A lone `[windows]` recipe makes `just` on Linux/macOS report "no recipe found".
- `[unix]` is a real attribute in just (covers Linux and macOS). Do not use `[linux]` and assume macOS is included.
- bque is a Windows-only global kernel lock. The `[unix]` branch running the raw command is the intended behavior — never call `bque` there.
- If a recipe is identical on all platforms (e.g. `fmt`), write it once with no attribute.

## 3. Arguments and shell details

- Variadic recipes: `build *ARGS:` then forward with `{{ARGS}}`. In a template file, wrap that variable in a minijinja raw block so it survives rendering.
- Prefix the command with `@` to suppress just's command echo when the recipe itself prints useful output.
- When a layer uses worktrees, inject the shared target dir through `pengj-templates-cli worktree target-dir` — see `templates/rust/.pengj-templates/just/rust.just` for the `shared_win()` / `shared_sh()` macro pattern. Do not hardcode `CARGO_TARGET_DIR`.
- Multi-line recipes: just runs each line in its own shell by default. Use `#!/usr/bin/env bash` shebang recipes or explicit line continuation when state must be shared.

## 4. Managed-block discipline

- Root `justfile`: the framework inside `# PENGJ_TEMPLATE_START/END` belongs to the `just` layer; anything outside is the project's and survives `update`. Never move project recipes into the block.
- `.pengj-templates/just/*.just`: full-overwrite managed files. To change one, edit its source under `templates/<layer>/.pengj-templates/just/` and re-render; do not patch it in a generated project.
- Adding a layer module: add a matching conditional `import` in the root `justfile`'s managed block and add the layer as a dependency of `just` in the module's layer metadata.

## 5. No-bque downgrade (legitimate)

When `bque` is not among `layers`, write the direct command with a short comment explaining that the project has no global queue:

```just
# Direct build: this project does not select the bque layer.
build *ARGS:
    cargo build {{ARGS}}
```

This is the current layer's legitimate downgrade, not the recommended end state. If heavy parallel builds later cause CPU contention or lock collisions, the fix is to add the `bque` layer and re-wrap the recipes — not to keep the raw command.

