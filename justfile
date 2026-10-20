set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# List the recipes.
default:
    @just --list

# Run the Shell (the Dev host on Windows).
run:
    cargo run -p m4tv-shell

# Compile everything, as CI does.
build:
    cargo build --workspace --all-targets --locked

# Run the tests.
test:
    cargo test --workspace --locked

# Check formatting and lints (needs the rustfmt and clippy components).
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

# What CI runs.
ci: build test
