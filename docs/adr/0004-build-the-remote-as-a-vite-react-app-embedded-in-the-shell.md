# Build the Remote as a Vite, React and TypeScript app embedded in the Shell

The Remote is a Vite, React and TypeScript app in `packages/remote`, and the Shell binary embeds its built files and serves them from the Remote server's origin. We chose it over Rust-compiled-to-WASM because the Remote is a thin phone client whose contract is the Remote server's protocol, which the Rust protocol tests already pin. Rust-everywhere stays the rule for the device (the Shell, the Remote server and OS glue); the Remote is the second non-Rust component, after the Engine.

## Decisions

- **Stack**: React and React DOM, `@noble/hashes` for the Pairing handshake, and Tailwind CSS compiled to one static stylesheet. Hand-built components, no UI kit, no router (one screen with a tab bar) and no state library. Vite, TypeScript and Vitest are dev-only.
- **Targets**: es2020 and iOS 14+, with no legacy build. A phone or captive-portal mini-browser that cannot run it gets the fallback below.
- **Fallback**: `/` serves a small hand-written HTML page that needs no JavaScript and tells the viewer to open the address in their browser (address and QR), then loads the app. Which phones need it is a real-device test (map: captive-portal mini-browser).
- **Build**: `build.rs` runs `npm ci` and `npm run build` for `packages/remote`, skipping npm when the sources and lockfile are unchanged, and `rust-embed` embeds `dist/`. A Rust build fails with a clear error when Node is missing, unless `M4TV_SKIP_WEB=1` is set, which embeds a "Remote not built" page. CI sets up Node.
- **Protocol types**: the TypeScript types are generated from the Rust types with `ts-rs` into `packages/remote/src/protocol/`, committed, and a Rust test fails when they are stale. The Rust types stay the single source of truth, which "adding a setting adds it to the Remote with no extra Remote code" relies on.
- **Tests**: Vitest covers the logic that is not UI (Pairing handshake, WebSocket reconnect, mapping live state to screens). The UI is checked on a real phone.

## Considered Options

- **Rust to WASM (Leptos, Yew, Dioxus)**: keeps Rust-everywhere and could share types with the server. Rejected because it needs the wasm32 target and trunk in CI and the image build, ships a bigger payload, and WASM in a captive-portal mini-browser is unverified.
- **Plain JavaScript with no build step**: smallest and lowest risk. Rejected in favour of typed React, accepting the larger bundle and a Node toolchain.
- **Loose files in the image**: nothing to rebuild when the Remote changes. Rejected because it adds a second artifact that must match the server's protocol version and a path the Dev host and the image must agree on.
- **`build.rs` that warns and embeds a placeholder when Node is missing**: rejected because a release artifact could ship with no Remote and nobody would notice.

## Consequences

- Every cargo build needs Node and npm unless `M4TV_SKIP_WEB=1` is set, and the Shell and Remote ship as one artifact: a Remote change means rebuilding the Shell.
- The device image still never builds anything: Node is needed where the Shell artifact is built, not on the Pi.
- The repo gains a Node toolchain and a JavaScript supply chain, kept small by the dependency list above.
- Old phones and some mini-browsers will not run the app; the fallback page is their way on.
