# just/rust.just — common tasks for a single-crate Rust project (managed by the rust layer)
# Imported by the root justfile based on the selected layers. If the project has its own
# orchestration, set options["just_recipes"] = "none" and keep these recipes in the project-owned area.

# Build (Windows: global queue + Idle priority)
[windows]
build *ARGS:
    @bque run -- cargo build {{ARGS}}
[unix]
build *ARGS:
    cargo build {{ARGS}}

# Check (clippy with all features, warnings are errors)
[windows]
check *ARGS:
    @bque run -- cargo clippy --all-targets --all-features -- -D warnings {{ARGS}}
[unix]
check *ARGS:
    cargo clippy --all-targets --all-features -- -D warnings {{ARGS}}

# Test
[windows]
test *ARGS:
    @bque run -- cargo test --all-features {{ARGS}}
[unix]
test *ARGS:
    cargo test --all-features {{ARGS}}

# Benchmark (suspends background builds for a clean CPU environment)
[windows]
bench *ARGS:
    @bque bench -- cargo bench {{ARGS}}
[unix]
bench *ARGS:
    cargo bench {{ARGS}}


# Format code (lightweight, no queue)
fmt:
    cargo fmt

# Format check (CI only)
fmt-check:
    cargo fmt --check

# Full CI check (each sub-command takes the lock separately; bque handles re-entrancy)
ci: fmt-check check test