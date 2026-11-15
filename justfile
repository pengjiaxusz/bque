# justfile — unified task entry point (managed by the just layer)
# The framework area below is a managed block: template updates replace it in place; everything outside the block belongs to the project.
# Heavy commands go through `just q-run <cmd>`; benchmarks go through `just q-bench <cmd>`.
# PENGJ_TEMPLATE_START
[unix]
set shell := ["sh", "-uc"]
[windows]
set shell := ["pwsh", "-NoProfile", "-Command"]


import 'just/bque.just'


import 'just/rust.just'


# List all available recipes
default:
    @just --list
# PENGJ_TEMPLATE_END

# ---------------------------------------------------------------------------
# Project-owned area: template updates only replace the managed block above. This area belongs to the project and is preserved in full.
# Register project recipes here; new heavy tasks must call `just q-run <cmd>` / `just q-bench <cmd>`.
# Never run cargo / cmake / ninja / pnpm build / tsc or other compilers raw.
# ---------------------------------------------------------------------------