# justfile — unified task entry point (managed by the just layer)
# The framework area below is a managed block: template updates replace it in place; everything outside the block belongs to the project.
# Heavy commands go through `just q-run <cmd>`; benchmarks go through `just q-bench <cmd>`.
# PENGJ_TEMPLATE_START
[unix]
set shell := ["sh", "-uc"]
[windows]
set shell := ["pwsh", "-NoProfile", "-Command"]
# Let the project area override a framework recipe with the same name (e.g. a `test` that pre-builds an artifact), instead of editing the managed imports.
set allow-duplicate-recipes := true


import '.pengj-templates/just/bque.just'


import '.pengj-templates/just/gate.just'


import '.pengj-templates/just/rust.just'


# List all available recipes
default:
    @just --list

# Environment check: toolchain + commit hooks + agent workspace links (extend with .pengj-templates/scripts/doctor.d/)
[unix]
doctor *ARGS:
    @sh .pengj-templates/scripts/doctor.sh {{ARGS}}
# Environment check: toolchain + commit hooks + agent workspace links (extend with .pengj-templates/scripts/doctor.d/)
[windows]
doctor *ARGS:
    @pwsh -NoProfile -File .pengj-templates/scripts/doctor.ps1 {{ARGS}}
# PENGJ_TEMPLATE_END

# ---------------------------------------------------------------------------
# Project-owned area: template updates only replace the managed block above. This area belongs to the project and is preserved in full.
# Register project recipes here; new heavy tasks must call `just q-run <cmd>` / `just q-bench <cmd>`.
# Never run cargo / cmake / ninja / pnpm build / tsc or other compilers raw.
# ---------------------------------------------------------------------------