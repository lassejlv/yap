set shell := ["bash", "-cu"]
set dotenv-load := true

default: run

run:
    RUST_LOG=${RUST_LOG:-whispr=info} cargo run --release

dev:
    RUST_LOG=${RUST_LOG:-whispr=debug} cargo run

build:
    cargo build --release

check:
    cargo check --all-targets

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets -- -D warnings

clean:
    cargo clean

bundle:
    cargo build --release
    @echo "Binary at target/release/whispr"
