# justfile — unified task entry point (managed by the just layer)
# The framework area below is a managed block: template updates replace it in place; everything outside the block belongs to the project.
# Heavy commands call `bque run -- <cmd>`; benchmarks call `bque bench -- <cmd>` (the bque layer ships no wrapper).
# PENGJ_TEMPLATE_START
set minimum-version := "1.56.0"
[unix]
set shell := ["sh", "-uc"]
[windows]
set shell := ["pwsh", "-NoProfile", "-Command"]
[unix]
export PATH := env('XDG_DATA_HOME', env('HOME', '') + '/.local/share') + '/pengj-templates/bin:' + env('HOME', '') + '/.local/bin:' + env('PATH', '')
# Let the project area override a framework recipe with the same name (e.g. a `test` that pre-builds an artifact), instead of editing the managed imports.
set allow-duplicate-recipes := true


import '.pengj-templates/just/gate.just'


import '.pengj-templates/just/rust.just'


# List all available recipes
default:
    @just --list

# Environment check: toolchain + commit hooks + agent workspace links (extend with .pengj-templates/scripts/doctor.d/)
doctor *ARGS:
    @pengj-templates-cli doctor --dir . {{ARGS}}
# PENGJ_TEMPLATE_END

# ---------------------------------------------------------------------------
# Project-owned area: template updates only replace the managed block above. This area belongs to the project and is preserved in full.
# Register project recipes here; new heavy tasks call `bque run -- <cmd>` / `bque bench -- <cmd>` (no ad-hoc wrapper exists).
# Never run cargo / cmake / ninja / pnpm build / tsc or other compilers raw.
# ---------------------------------------------------------------------------