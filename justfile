# Bypass is for this process only: the VS developer shell script is unsigned, and the default
# policy refuses to run scripts.
set windows-shell := ["powershell.exe", "-NoLogo", "-ExecutionPolicy", "Bypass", "-Command"]

# List the recipes.
default:
    @just --list

# The first build downloads CEF into ~/.cache/m4tv-cef (unless CEF_PATH is set) and compiles its
# wrapper, which needs cmake, ninja and MSVC: on Windows this opens the Visual Studio developer
# environment unless you are already in one.

# Run the Shell with the Engine (the Dev host on Windows). Building it builds the Remote web app
# first (crates/shell/build.rs, which needs Node; set M4TV_SKIP_WEB=1 to go without).
[windows]
run:
    if (-not $env:VSCMD_VER) { \
        $vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath; \
        & "$vs\Common7\Tools\Launch-VsDevShell.ps1" -Arch amd64 -SkipAutomaticLocation \
    }; \
    if (-not $env:CEF_PATH) { $env:CEF_PATH = "$HOME\.cache\m4tv-cef" }; \
    cargo run -p m4tv-shell --features cef

[unix]
run:
    cargo run -p m4tv-shell --features cef

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
