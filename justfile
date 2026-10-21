# Bypass is for this process only: the VS developer shell script is unsigned, and the default
# policy refuses to run scripts.
set windows-shell := ["powershell.exe", "-NoLogo", "-ExecutionPolicy", "Bypass", "-Command"]

# List the recipes.
default:
    @just --list

# Download the pinned castLabs Electron (ADR 0006) into .m4tv-dev/electron, once. It is the Engine
# the Dev host runs; it needs Node.
engine:
    node engine/fetch-electron.mjs

# Run the Shell with the Engine (the Dev host on Windows). Building it builds the Remote web app
# first (crates/shell/build.rs, which needs Node; set M4TV_SKIP_WEB=1 to go without).
run: engine
    cargo run -p m4tv-shell

# Compile everything, as CI does.
build:
    cargo build --workspace --all-targets --locked

# Run the tests: the Rust ones, then the Remote web app's (installed by the Rust build).
test:
    cargo test --workspace --locked
    npm test --prefix packages/remote

# Check formatting and lints (needs the rustfmt and clippy components).
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings

# What CI runs.
ci: build test
