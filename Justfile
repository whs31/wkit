alias fmt := format

format:
    cargo +nightly fmt --all

lint:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

package-arch:
    cargo run --release --locked -- package arch --output target/package

package-deb:
    cargo run --release --locked -- package deb --output target/package
