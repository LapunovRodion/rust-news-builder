# Development tasks for rust-news-builder.
#
# `just gates` is the merge bar: the four checks the constitution's Development Workflow
# section requires to be green before anything lands.

default:
    @just --list

# The four merge gates, in the order that fails fastest.
gates: fmt-check clippy test parity ui-check

# The frontend's own gates: types and the body round trip (T108).
ui-check:
    cd crates/desktop/ui && pnpm install --frozen-lockfile && pnpm check && pnpm test

# Build the frontend bundle the Tauri build embeds.
ui-build:
    cd crates/desktop/ui && pnpm install --frozen-lockfile && pnpm build

# Formatting, as a check rather than a rewrite.
fmt-check:
    cargo fmt --all -- --check

# Rewrite formatting in place.
fmt:
    cargo fmt --all

# Lints, with warnings treated as errors.
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# The whole test suite.
test:
    cargo test --workspace

# The parity suite on its own: the fixtures captured from the Python reference.
parity:
    cargo test -p newsbuilder-core --test parity_render --test parity_tables \
        --test parity_text --test parity_paths --test parity_encode

# Invariants from data-model.md.
invariants:
    cargo test -p newsbuilder-core --test model_invariants

# Regenerate fixtures/inputs/ and fixtures/reference/ from the pinned Python reference.
# Commit the result: the goldens are checked in so the test suite needs no Python.
capture:
    cargo run -p capture-reference

# Fail if the committed goldens are stale.
capture-check:
    cargo run -p capture-reference -- --check

# The desktop application, in development mode.
desktop:
    cargo tauri dev

# The opt-in end-to-end publish test, which needs a local sshd.
e2e:
    NEWSBUILDER_E2E_SSH=1 cargo test -p newsbuilder-core --features sftp --test e2e_sshd -- \
        --ignored --nocapture

# The SC-008 preview latency measurements (T102). Optimised, because timings in a debug build
# measure the debug build. `--nocapture` so the numbers are printed, not just the verdict.
# `--test-threads=1` matters: cargo runs tests concurrently, and three benchmarks each
# encoding thirty photos across every core measure contention rather than latency.
bench-preview:
    cargo test -p newsbuilder-core --release --test preview_latency -- \
        --ignored --nocapture --test-threads=1
