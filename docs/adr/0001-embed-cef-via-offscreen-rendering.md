# Embed CEF via off-screen rendering into gpui

**Superseded by [ADR 0006](0006-run-the-engine-as-castlabs-electron-in-a-child-process.md)**: the off-screen idea stands, but the Engine is now castLabs Electron in a child process, because compiling our own CEF with H.264/AAC was not wanted.

The Engine is the CEF library, embedded in the Shell, with its frames rendered off-screen into gpui, because the Shell must draw Overlays on a playing App and keep keyboard focus, which separate Cage windows cannot do (Cage has no layer-shell and no way to raise or focus a window). We start with CPU copies into a gpui image and fork `gpui-pre-wgpu` for dmabuf import only if a Pi 4 spike misses the frame budget.

## Considered Options

- **Separate Cage windows** (Engine in its own window, Home swaps screens): no frame copies, but no Overlays on a playing App. Rejected only because Overlays are required.
- **WPE WebKit**: no evidence of Widevine working on a Pi.
- **Raspberry Pi's Chromium driven over DevTools Protocol**: best hardware H.264 path on Pi 4, but it is a separate window, so no Overlays.

## Consequences

- We build our own CEF for linuxarm64 with H.264/AAC, because official builds lack them.
- Every frame costs at least three full-frame CPU copies until the dmabuf fork exists, so Pi 4 frame rate is unproven.
- We own a key-to-virtual-key mapping and manual disposal of old frames.
