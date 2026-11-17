# .pengj-templates/just/gate.just — commit gate recipes (managed by the gate layer; template updates overwrite this file)
# The commit gate ships inside `pengj-templates-cli`, so the check recipes below call `pengj-templates-cli gate ...`. Activate the hooks with `just doctor --fix` (or `sh .pengj-templates/scripts/doctor.sh --fix` when the CLI is not installed) — the environment check is the single activation entry point. `gate-hooks-local` is the raw zero-dependency script it calls.

# Run the code language guard over the whole working tree
gate-check:
    @pengj-templates-cli gate check

# Raw activation: `sh .githooks/install.sh` points core.hooksPath at .githooks and records the executable bit using only git and a POSIX shell. Prefer `just doctor --fix`; this recipe is the bare entry it calls.
gate-hooks-local:
    @sh .githooks/install.sh

# Check generated docs follow the language axis: en projects contain no Han, zh projects' human-facing copy contains Chinese. Acts only on template-managed files from the manifest.
gate-lang:
    @pengj-templates-cli check-lang --dir .

# Run the built-in rule self-test
gate-selftest:
    @pengj-templates-cli gate selftest