---
name: write-just
description: >-

  Writes and audits the root justfile and `.pengj-templates/just/*.just` layer modules so heavy commands stay under the bque queue: correct file locations, Windows/Unix paired recipes, and mandatory wrappers for cargo run/build/clippy/test/bench. Use when adding or editing a just recipe, wiring a new layer's just module, or seeing a raw `cargo run` / `cargo build` in a justfile.
  Triggers: justfile, just recipe, .just, bque, bque run, cargo run, PENGJ_TEMPLATE_START, [windows], [unix], import.

---

<!-- PENGJ_TEMPLATE_START -->

# write-just — justfile & layer recipe authoring

A project's command surface lives in two places only: the single root `justfile` and the managed modules under `.pengj-templates/just/`. Every heavy command goes through the bque queue on Windows. This skill keeps new recipes inside those boundaries — the usual failure is a hand-written `cargo run` that quietly bypasses the queue.

## 1. Where a recipe belongs

| What you are writing | Where it goes | Who owns it |
| :--- | :--- | :--- |
| Entry point, `set shell`, `default`, conditional `import` | Root `justfile`, inside `# PENGJ_TEMPLATE_START/END` | `just` layer, replaced in place on `update` |
| Project-only recipes | Root `justfile`, **outside** the managed block | Project, never overwritten |
| Recipes shared by projects selecting a layer | `.pengj-templates/just/<layer>.just` | That layer, full overwrite on `update` |

Never create root `scripts/` or root `just/` — those names belong to the project. Do not hand-edit a managed `.just` module in a generated project; change the template and re-run `update`.

## 2. Four hard rules

1. **Wrap every heavy command when `bque` is in `layers`** — including `cargo run`. Use `bque run -- <cmd>`; benchmarks use `bque bench -- <cmd>`. A raw `cargo run/build/clippy/test/bench`, `ninja`, `cmake --build`, `ctest`, or `pnpm build` is a defect.
2. **No bque layer?** A direct command is the legitimate downgrade of the current layer — say so in a comment, and do not present it as the recommended endpoint. Never force raw `bque` into Unix/CI.
3. **Platform pairs**: if Windows and Unix differ, write both `[windows]` and `[unix]` recipes. bque supports both Windows and Linux/WSL2, while generated `[unix]` recipes intentionally pass through so CI runners stay zero-dependency — that is expected, not a bypass.
4. **Variadic args**: declare `*ARGS` and forward with `{{ARGS}}`. There is no ad-hoc wrapper recipe (`just q-run` is gone): one-off heavy commands call `bque run -- <cmd>` / `bque bench -- <cmd>` directly, and a command that keeps coming back gets its own recipe here.

## 3. Minimal correct recipe

```just
# Run the binary (Windows: global queue + Idle priority)
[windows]
run *ARGS:
    @bque run -- cargo run {{ARGS}}
[unix]
run *ARGS:
    cargo run {{ARGS}}
```

Wrong — do not write this:

```just
run *ARGS:
    cargo run {{ARGS}}   # bypasses bque on Windows
```

## 4. Editing a template `.just` file

Human-facing comments must carry both language branches (`{% if options["skill_lang"] | default("zh") == "en" %} … `{% else %} … `{% endif %}`), and the rendered zh output must stay byte-identical to the previous version. Literal just variables such as `{{ARGS}}` must be wrapped in a minijinja raw block — copy the exact form from the examples above or any existing `.just` template. Gate imports on `layers`; keep the module header naming the owning layer.

Full command mapping, more pitfalls, and the no-bque fallback: [references/command-map.md](references/command-map.md).

## 5. Self-check

```sh
sh .agents/skills/write-just/scripts/check-just.sh          # scan justfile + managed modules
sh .agents/skills/write-just/scripts/check-just.sh --dir .  # explicit project root
```

The script is read-only and exits non-zero when it finds an unwrapped heavy command. Fix its findings before finishing; a finding is a real defect, not a style warning.


<!-- PENGJ_TEMPLATE_END -->


<!-- Project-specific area: template updates only replace the managed block above; this area is owned by the project. -->
## Project-Specific Recipe Conventions

> Record project-only heavy commands, custom queue policies, or recipes intentionally left unwrapped here.
