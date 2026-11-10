# The recipes check each feature on its own, without default features, and all features
# together, with `cargo hack --each-feature`. Cargo unifies features within one invocation, so
# code that silently relies on `tokio` only shows up when that feature is off.

# Lists all available commands.
default:
    just --list

# Install tools required by other recipes.
install-tools:
    rustup target add wasm32-unknown-unknown thumbv7em-none-eabihf
    rustup toolchain install 1.85.0 nightly --profile minimal
    rustup target add --toolchain 1.85.0 wasm32-unknown-unknown thumbv7em-none-eabihf
    cargo +stable install cargo-minimal-versions --locked
    # Used by cargo-minimal-versions.
    cargo +stable install cargo-hack --locked
    cargo +stable install cargo-semver-checks --locked
    cargo +stable install cargo-msrv --locked
    cargo +stable install cargo-audit --locked
    cargo +stable install cargo-sort --locked
    cargo +stable install cargo-llvm-cov --locked
    rustup component add llvm-tools-preview

# Check if the current dependency version bounds are sufficient, including for the tests.
minimal-versions:
    cargo minimal-versions test --direct --all-features
    # futures-timer has code of its own for the browser, which did not compile in 3.0.0.
    cargo minimal-versions check --direct --lib --no-default-features --features futures-timer \
        --target wasm32-unknown-unknown

# Find the minimum supported rust version.
msrv:
    cargo msrv find -- cargo check --lib

# Check whether current changes require a breaking release.
semver-checks:
    cargo semver-checks

# Check the formatting of the code and the order of the dependencies.
fmt-check:
    cargo fmt --all -- --check
    cargo sort --check

wasm := "--no-default-features --target wasm32-unknown-unknown --locked"

# Lint the parts of the crate that build for the browser.
check-wasm:
    cargo clippy --lib {{ wasm }} -- -D warnings
    cargo clippy --lib {{ wasm }} --features futures-timer -- -D warnings
    cargo clippy --lib {{ wasm }} --features fastrand -- -D warnings
    # With the default features, `std` and `tokio`, which a crate has unless it turns them off.
    cargo clippy --lib --target wasm32-unknown-unknown --locked -- -D warnings
    # Without its `js` feature, fastrand would use the same seed in every browser.
    cargo tree {{ wasm }} --features fastrand -e features -i fastrand \
        | grep -q 'fastrand feature "js"'
    # Without its `wasm-bindgen` feature, `FuturesTimerSleep` would compile for the browser and
    # fail at run time.
    cargo tree {{ wasm }} --features futures-timer -e features -i futures-timer \
        | grep -q 'futures-timer feature "wasm-bindgen"'

no_std := "--lib --no-default-features --target thumbv7em-none-eabihf --locked"

# Lint the crate without `std`, for a target without an operating system.
check-no-std:
    cargo clippy {{ no_std }} -- -D warnings
    cargo clippy {{ no_std }} --features alloc,embassy-time -- -D warnings

# Lint the code with every feature set.
clippy:
    cargo hack clippy --each-feature --all-targets --locked -- -D warnings

# Run all tests with every feature set.
test:
    cargo hack test --each-feature --locked

# Check the library on its MSRV, 1.85.0: on the host, on a microcontroller and in the browser.
check-msrv:
    RUSTFLAGS="-D warnings" cargo +1.85.0 check --lib --no-default-features --locked
    RUSTFLAGS="-D warnings" cargo +1.85.0 check --lib --locked
    RUSTFLAGS="-D warnings" cargo +1.85.0 check --lib --all-features --locked
    RUSTFLAGS="-D warnings" cargo +1.85.0 check {{ no_std }} --features alloc,embassy-time
    RUSTFLAGS="-D warnings" cargo +1.85.0 check --lib {{ wasm }} --features futures-timer

# Build the docs as docs.rs does, on nightly with all features, and run the doctests there.
docs-rs:
    RUSTDOCFLAGS="--cfg docsrs -D warnings" cargo +nightly doc --all-features --no-deps --locked
    # Stable ignores the error code of a `compile_fail,E0277` doctest, so that the doctest passes
    # when its code fails to compile for another reason. Nightly checks the code.
    cargo +nightly test --doc --all-features --locked

# Build and open the crate documentation for review.
open-docs:
    cargo doc --all-features --no-deps --open

# Check that the crate packages and its package builds. Allows uncommitted changes.
package:
    cargo package --locked --allow-dirty

# Measure the test coverage with all features, and write it to `lcov.info`, which CI uploads to
# Codecov.
coverage:
    cargo llvm-cov --all-features --locked --lcov --output-path lcov.info

# Show the lines that the tests do not cover.
coverage-missing:
    cargo llvm-cov --all-features --locked --show-missing-lines

# Run security auditing.
audit:
    cargo audit --deny warnings

# Update all deps; sort all Cargo.toml deps; format all code.
tidy:
    cargo update
    cargo sort
    cargo fmt --all

# Run the full non-mutating validation suite, which CI runs too.
verify:
    just fmt-check
    just check-wasm
    just check-no-std
    just clippy
    just test
    just check-msrv
    just docs-rs
    just package
    just minimal-versions
    just audit
    just semver-checks
