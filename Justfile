alias fmt := format

format:
    cargo +nightly fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace