# .pengj-templates/just/gate.just — commit gate recipes (managed by the gate layer; template updates overwrite this file)
# The commit gate ships inside `pengj-templates-cli`, so the check recipes below call `pengj-templates-cli gate ...`. Activate the hooks with `just doctor --fix` (or `pengj-templates-cli gate hooks install`) — the built-in doctor is the unified activation entry point.

# Run the code language guard over the whole working tree
gate-check:
    @pengj-templates-cli gate check

# Check generated docs follow the language axis: en projects contain no Han, zh projects' human-facing copy contains Chinese. Acts only on template-managed files from the manifest.
gate-lang:
    @pengj-templates-cli check-lang --dir .

# Run the built-in rule self-test
gate-selftest:
    @pengj-templates-cli gate selftest