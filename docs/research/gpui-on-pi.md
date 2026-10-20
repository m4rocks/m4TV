# Does gpui run well on Pi 4 and Pi 5 as a kiosk shell?

Research for GitHub issue m4rocks/m4TV#4. Date of research: 2026-10-20.

Question: can gpui (outside the Zed repo) render on Raspberry Pi 4 and Pi 5 under a Wayland kiosk compositor with no desktop? Linux backends, Mesa v3dv, standalone-crate usability, stability and API churn, performance on low-end GPUs.

Source snapshot: `zed-industries/zed` main at commit `b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c` (2026-10-20). Permalinks below use `Z = https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c`. Nothing was run on a Pi. Everything below is read from source, docs and issue threads, not measured.

## Short answer

- **Pi 5: probably yes, never confirmed in a kiosk.** Zed (the full editor, built on gpui) is reported working on Pi 5 on desktop Wayland since mid-2026. No source shows gpui under cage or any other kiosk compositor.
- **Pi 4: unproven.** The only Pi 4 report in Zed's tracker is a VNC glitching bug from 2025 (blade era). No one reports the current renderer on a Pi 4.
- **Use a git dependency on Zed, not the `gpui` crate on crates.io.** The crates.io `gpui` is a year old and uses an older renderer. The Linux Wayland and wgpu crates are not published at all.
- **API churn is high.** gpui says it is pre-1.0 with frequent breaking changes, and its main branch still gets breaking renames.
- **Performance on Pi-class GPUs is not measured anywhere.** The only evidence is user reports of tearing and flicker on Pi 5 in the full editor.

## 1. Linux backends

### 1.1 Current architecture (git main) uses wgpu, not Blade

- Linux windowing lives in a separate crate `gpui_linux`, with `wayland` and `x11` features, both on by default. [Z/crates/gpui_linux/Cargo.toml](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_linux/Cargo.toml)
- The renderer is `gpui_wgpu`. On Linux/FreeBSD it enables wgpu's `vulkan` and `gles` backends. [Z/crates/gpui_wgpu/Cargo.toml](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/Cargo.toml)
- At runtime the wgpu instance is created with `Backends::VULKAN | Backends::GL`. [Z/crates/gpui_wgpu/src/wgpu_context.rs L476](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_context.rs#L476) So there is a GL/EGL fallback if Vulkan fails.
- Adapter choice: all adapters are enumerated and sorted by user override (`ZED_DEVICE_ID`), then the GPU the compositor uses, then device type (discrete > integrated > other > virtual > CPU), then Vulkan over GL. Each candidate is tested by configuring the real surface. [wgpu_context.rs L596](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_context.rs#L596)
- Device limits are wgpu `downlevel_defaults()` widened to the adapter's resolution/alignment limits, so the demand on the GPU is low. [wgpu_context.rs L448](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_context.rs#L448)
- Dual-source blending is optional. Without it, subpixel text antialiasing is disabled with a warning. [wgpu_context.rs L424](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_context.rs#L424)
- The wgpu dependency is a git pin to `gfx-rs/wgpu` rev `3b3506ea31154edc04de64debda0e780cba2639c`. [Z/Cargo.toml L991](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/Cargo.toml#L991)

**Older crates.io version differs.** `gpui` 0.2.2 (2025-10-22) uses `blade-graphics` for the Linux `wayland` and `x11` features, not wgpu. Source: the published tarball `https://static.crates.io/crates/gpui/gpui-0.2.2.crate`, Cargo.toml feature table. The Pi bug reports from late 2025 (see 3.2) ran on that Blade renderer (log line `gpui::platform::blade::blade_renderer` in [zed#45989](https://github.com/zed-industries/zed/issues/45989)).

### 1.2 Wayland backend: what it needs from the compositor

From `gpui_linux/src/linux/wayland/client.rs`: [client.rs](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_linux/src/linux/wayland/client.rs)

- **Required globals:** `wl_compositor`, `wl_shm`, and `xdg_wm_base` (bound at version 1 to 6; "older compositors bind at their own version"). Missing any of them is a hard error (L276-301).
- **Optional globals** (bound with `.ok()`, so absence is tolerated): `wp_viewporter`, `wp_fractional_scale_manager_v1`, `zxdg_decoration_manager_v1`, `zwlr_layer_shell_v1` (v1-5), cursor-shape, primary selection, data device, text-input, activation, gestures, blur, system bell (L302-310).
- Window decorations default to client-side and switch to server-side if the compositor offers them. Fullscreen is requested through `xdg_toplevel.set_fullscreen`. [window.rs L1949](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_linux/src/linux/wayland/window.rs#L1949)
- The client is also wired for `wlr-layer-shell`, which could suit a kiosk shell on compositors that offer it. There is a `layer_shell.rs` example. [Z/crates/gpui/examples/layer_shell.rs](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui/examples/layer_shell.rs)
- The renderer asks for `PresentMode::Mailbox` and falls back to `Fifo` if the surface lacks it. [wayland/window.rs L585](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_linux/src/linux/wayland/window.rs#L585), [wgpu_renderer.rs L459](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_renderer.rs#L459)
- The XDG desktop-portal listener (D-Bus) is best-effort. If it fails, it logs at debug level and stops. So a bare image without a portal or session bus should not abort startup. [xdg_desktop_portal.rs](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_linux/src/linux/xdg_desktop_portal.rs) (I did not run it without D-Bus.)
- Fonts: the Linux text system uses cosmic-text and `font-kit` to match system fonts, and apps can add fonts with `add_fonts`. A kiosk image therefore needs fontconfig and a font, or embedded fonts. [cosmic_text_system.rs](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/cosmic_text_system.rs)

### 1.3 Fit with Cage (the likely kiosk compositor)

- Cage is a wlroots-based kiosk that "runs a single, maximized application". Built against wlroots 0.20. Latest release v0.3.1 (2026-06-30). [cage README](https://github.com/cage-kiosk/cage/blob/master/README.md), [meson.build](https://github.com/cage-kiosk/cage/blob/master/meson.build)
- Cage's source creates: `wlr_xdg_shell` at version 5, `wlr_xdg_decoration_manager_v1`, `wlr_viewporter`, `wlr_cursor_shape_manager_v1`, linux-dmabuf, primary selection, data device. [cage.c](https://github.com/cage-kiosk/cage/blob/master/cage.c) That covers gpui's three required globals. I did not see fractional-scale or layer-shell in the lines I read. Both are optional for gpui.
- **Open risk, wlroots flicker:** Zed has a long-running report of rapid flicker on wlroots compositors (Sway), from 2025-06 to 2026-05. Causes named in the thread: explicit-sync handling in wlroots, fixed by wlroots MR !4979 (merged to master 2026-03-19). A Hyprland user still saw it after that. [zed#32792](https://github.com/zed-industries/zed/issues/32792), [wlroots MR 4979](https://gitlab.freedesktop.org/wlroots/wlroots/-/merge_requests/4979). Workarounds in the thread: run under XWayland (`env -u WAYLAND_DISPLAY`), or set `WLR_RENDER_NO_EXPLICIT_SYNC=1`.
- I could not confirm that wlroots 0.20.x (tagged 2026-03-26 onward) contains that fix or that Cage 0.3.1 is free of the flicker on a Pi. Treat as unverified until tested.

## 2. Mesa v3dv (the Vulkan driver for Pi 4 and Pi 5)

- Mesa's own docs: V3DV is "a Vulkan graphics driver ... notably used on the Raspberry Pi 4 and Raspberry Pi 5"; development targets V3D 4.2 (Pi 4) and V3D 7.1 (Pi 5). The same page claims conformance only for the GLES driver ("conformant GLES3.1"); it makes no Vulkan conformance claim. [docs.mesa3d.org/drivers/v3d.html](https://docs.mesa3d.org/drivers/v3d.html)
- v3dv source (`src/broadcom/vulkan/v3dv_device.c`) advertises Vulkan 1.2 in Mesa 24.2 and Vulkan 1.3 from Mesa 25.0 through 26.0 and main. The same source also lists `KHR_wayland_surface` and `dynamicRendering = true` on main. Source: [mesa-24.2.0](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-24.2.0/src/broadcom/vulkan/v3dv_device.c), [mesa-25.0.7](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-25.0.7/src/broadcom/vulkan/v3dv_device.c), [main](https://gitlab.freedesktop.org/mesa/mesa/-/blob/main/src/broadcom/vulkan/v3dv_device.c). Advertised is not the same as conformance-tested at 1.3; I found no primary Khronos listing for that.
- **Dual-source blending** (`dualSrcBlend`): `false` in Mesa 25.0.7 and 25.1.0; `true` in Mesa 25.2.0, 25.3.0, 26.0.0 and main (same file, tags `mesa-25.1.0`, `mesa-25.2.0`). So on an older Mesa (Raspberry Pi OS ships 25.0.x per [zed#45989](https://github.com/zed-industries/zed/issues/45989), user report "Mesa 25.0.7-2+rpt4") gpui falls back to no subpixel text AA; on Mesa 25.2+ it uses it. This only changes text quality, not whether it runs.
- **Device-creation failure on v3dv, fixed upstream:** wgpu requested `shaderDrawParameters` unconditionally, and v3dv reports `shaderDrawParameters = false` (v3dv_device.c L404 on main). Device creation failed on Pi 5. wgpu fixed this in v29.0.2 (2026-05-01), "avoiding device creation failures on drivers that don't support it (e.g. V3DV, SwiftShader)". [wgpu v29.0.2 release](https://github.com/gfx-rs/wgpu/releases/tag/v29.0.2), PR [gfx-rs/wgpu#9331](https://github.com/gfx-rs/wgpu/pull/9331). Zed's pinned wgpu rev (2026-09-12 commit) is newer than that release, so it should include the fix. [wgpu commit 3b3506e](https://github.com/gfx-rs/wgpu/commit/3b3506ea31154edc04de64debda0e780cba2639c). That wgpu bug is what broke Zed on Pi 5 between 2026-03 and 2026-05 ([zed#45989](https://github.com/zed-industries/zed/issues/45989), [zed-industries/wgpu#1](https://github.com/zed-industries/wgpu/pull/1)).
- GL fallback exists too: Mesa describes the V3D GLES driver as conformant to GLES 3.1 (page above), and wgpu's GL backend needs GL ES 3.0+. [wgpu README](https://github.com/gfx-rs/wgpu/blob/trunk/README.md). I did not test whether gpui's GL path works on the Pi.

## 3. Evidence on actual Pi hardware

All from the Zed issue tracker (first-party, but user-reported):

### 3.1 Pi 5, works now (full Zed editor, desktop Wayland)
- zed#45989 (open): crash/glitch on Pi 5 since Zed 0.207; wgpu patch discussed 2026-04; then "Zed v1.3.5 official release works on Pi 5" (2026-05-22), with "significant tearing and flicker when scrolling and hovering over controls" (CM5 user, 2026-05-22), and "version 1.16.1 ... now works great on my Raspberry Pi 5 16GB" (2026-08-24). A Pi 500+ user on Fedora 44, Mesa 26.0.5, GNOME Shell 50 saw no flicker on Zed 1.0.1 under Wayland or X11 (2026-05-05). The issue is still open. [zed#45989](https://github.com/zed-industries/zed/issues/45989)
- zed#53683 (closed as duplicate, 2026-04): regression in 0.230.0 on Pi 5 Trixie; 0.228 and 0.229 worked. [zed#53683](https://github.com/zed-industries/zed/issues/53683)

### 3.2 Pi 4, no good evidence
- zed#43088: Pi 4 (8 GB), Raspberry Pi OS 64-bit, Zed 0.213.3 over RealVNC glitches the whole display while open. Closed as duplicate, no fix described. [zed#43088](https://github.com/zed-industries/zed/issues/43088)
- No issue reports the current wgpu renderer on V3D 4.2. A search of Zed issues for "V3D 4.2", "BCM2711" and "VideoCore VI" returned nothing relevant. Pi 4 is unverified with the current code.

### 3.3 Earlier Blade-era corruption (Pi 500/5/4-class, 2025-10 to 2026-02)
- zed#39806 (closed "not planned", 2026-02-23): Zed 0.207.3 on Pi corrupts video memory, persists after exit on Wayland, needs a reboot. A bisect points at the Linux perceptual gamma/contrast font-rendering change (#38862). Reports on Debian 12 and 13, Mesa 24.2.8, Pi 5 and Pi 500+, Wayland and X11 (the damage was recoverable on X11 per one user). Last substantive comment (2026-03-03, Zed nightly 0.227, CM5): it "starts up and looks good" but "the renderer can't quite keep up with the scrolling of the interface ... some of the characters are rendered and come in like rain". [zed#39806](https://github.com/zed-industries/zed/issues/39806)
- zed#41008, zed#41506, zed#47264: same symptom (green static, GPU unusable until reboot), closed as duplicates of 39806.

Takeaway: a GPU-wide corruption that survives the process is a bad failure mode for an unattended kiosk. It was tied to the old Blade path, but the replacement path has only been exercised by a few users.

## 4. Standalone usability

- **crates.io `gpui` is stale.** Latest is 0.2.2, published 2025-10-22; earlier versions 0.2.0/0.2.1 from October 2025. It still uses Blade on Linux (section 1.1). [crates.io/crates/gpui](https://crates.io/crates/gpui) (versions from the crates.io API.)
- **The current Linux/Wayland/wgpu stack is not on crates.io.** `gpui_platform`, `gpui_linux` and `gpui_wgpu` do not exist on crates.io (API returns "does not exist"). Workspace default is `publish = false` ([Z/Cargo.toml L275](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/Cargo.toml#L275)); only `gpui` itself sets `publish = true`. [Z/crates/gpui/Cargo.toml](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui/Cargo.toml)
- The README on main tells apps to depend on `gpui` plus `gpui_platform` (features `wayland`, `x11`, `font-kit`) and start with `gpui_platform::application()`. [Z/crates/gpui/README.md](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui/README.md) A git dependency on `zed-industries/zed` pinned to a rev is therefore the only first-party route to the current backend.
- **Zed's workspace patches do not follow a git dependency.** Zed's root `Cargo.toml` has `[patch.crates-io]` for forks including `async-task`, `calloop` (gpui_linux uses `calloop = "0.14.3"`) and `khronos-egl` ([Z/Cargo.toml L1021+](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/Cargo.toml#L1021)). Cargo ignores `[patch]` in dependencies, so a downstream workspace would have to copy the relevant patches. I did not build a downstream project to verify which patches are required for a gpui-only build. Treat as unverified.
- The Zed repo pins a very new Rust toolchain (`channel = "1.98.1"`, [rust-toolchain.toml](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/rust-toolchain.toml)), and the README says to use "the latest version of stable Rust".
- Unofficial option: a third-party weekly snapshot `gpui-pre` (and `gpui-pre-platform`), versions 0.3.5 to 0.3.9 between 2026-09-14 and 2026-10-12, published on crates.io by user `huacnlee` (the gpui-component maintainer, not Zed). Description: "gpui-pre snapshot of zed@93d7456". The `gpui-component` workspace pins it exactly (`=0.3.9`), commenting that "any snapshot may change GPUI's API" and a caret requirement broke their build (#3156). [crates.io/crates/gpui-pre](https://crates.io/crates/gpui-pre), [gpui-component Cargo.toml](https://github.com/longbridge/gpui-component/blob/main/Cargo.toml). Not a Zed-owned source; no ownership or review claims verified beyond the crates.io owner list.
- aarch64: Zed ships aarch64 Linux builds (glibc >= 2.35), so gpui compiles for aarch64. [Z/docs/src/linux.md](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/docs/src/linux.md) A known aarch64 build break on CPUs without full FP16 (the `gemm` crate used by the editor, not gpui) was fixed in PR #57923 ([zed#57770](https://github.com/zed-industries/zed/issues/57770)). That does not affect a gpui-only app.

## 5. Stability and API churn

- gpui's own README: "GPUI is still in active development ... still pre-1.0. There will often be breaking changes between versions." [Z/crates/gpui/README.md](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui/README.md)
- Activity: 38 commits touched `crates/gpui` in the 30 days to 2026-10-20 (GitHub commits API, path filter). One example of breaking change on 2026-10-20: "gpui: Rename `ScrollWheelEvent` to `ScrollEvent` ..." (#65805).
- Big structural change in the past year: Linux moved from in-tree `platform/linux` + Blade (crates.io 0.2.2) to separate `gpui_linux`/`gpui_wgpu`/`gpui_platform` crates on wgpu (compare section 1.1 with the 0.2.2 tarball). Anything written against 0.2.x would need porting.
- Docs are thin: "the best way to learn about these APIs is to read the Zed source code". (README, above.)
- Renderer robustness: the wgpu layer handles device loss (recreates the context, rejecting software adapters on retry). [wgpu_renderer.rs L2229](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_renderer.rs#L2229)
- The Pi 5 breakage windows (0.207 to about 1.3.5, and 0.230.0) show that a driver or wgpu change can break one board for months with no Zed-side CI on Pi hardware (a grep of `.github/workflows` and `.github/actions` for "raspberry" or "v3d" found nothing).

## 6. Performance on low-end GPUs

- There are no published benchmarks for gpui on V3D or any low-end GPU. I found no primary source with frame times, fps or memory numbers for Pi 4 or Pi 5.
- Design points from source: the renderer is a batched wgpu pipeline using `downlevel_defaults` limits, mailbox presentation when offered, and software (CPU) adapters ranked last, and software is rejected on device-loss recovery. [wgpu_context.rs](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/crates/gpui_wgpu/src/wgpu_context.rs). gpui_wgpu has `bench-support` Criterion benches (`layout_line`) that log an error when they run on a software adapter, but no Pi numbers are published.
- Anecdotal only: Pi 5 / CM5 users of the full editor report tearing and flicker while scrolling (2026-05-22) and a "renderer can't keep up" effect on a nightly (2026-03-03); one commenter guessed the second was software rendering, which is not confirmed (links in section 3). A kiosk UI that scrolls less text than the code editor may behave differently, but nothing measured shows that.
- `MESA_VK_DEVICE_SELECT`, `ZED_DEVICE_ID`, and `WAYLAND_DISPLAY=""` (force XWayland) are the documented knobs for GPU and display selection. [Z/docs/src/linux.md](https://github.com/zed-industries/zed/blob/b0f1e1cb2f4be4c7df054f96a2a2721aa878ba6c/docs/src/linux.md). Setting `RUST_LOG=trace` made the UI unusable on Pi 5 in one report ([zed#39806](https://github.com/zed-industries/zed/issues/39806)).

## 7. Unverified claims and gaps

1. No source shows gpui under Cage or any kiosk compositor on a Pi. All Pi 5 success reports are the full editor on desktop sessions (GNOME on Fedora; Raspberry Pi OS desktop).
2. No source shows current gpui (wgpu) running on a Pi 4 (V3D 4.2) at all.
3. Whether wlroots 0.20.x / Cage 0.3.1 includes the explicit-sync fix, and whether the Sway/Hyprland flicker affects gpui under Cage on a Pi, is unknown.
4. Vulkan 1.3 conformance of v3dv on Pi 4 / Pi 5 is not confirmed. Only that the driver advertises 1.3 in current Mesa. Pi OS Mesa versions were taken from user reports in the Zed tracker, not from Raspberry Pi's package pages.
5. Which Zed `[patch.crates-io]` entries a downstream git-dependency build needs was not tested.
6. The GL (EGL/GLES) fallback path on V3D was not checked in any report.
7. No performance numbers exist for Pi-class GPUs; the CPU cost, memory use and frame pacing under an animated kiosk UI are unknown.
8. I could not tell why zed#39806 was closed as "not planned" (the thread continues past the closure).

## 8. Suggested next step (not done here)

A small hardware spike would settle items 1-3 and 6-7 faster than more reading: build the `hello_world` example from the pinned Zed rev on a Pi 5 and a Pi 4, run it under `cage` from a TTY with Mesa 25.2 or newer, and record `Using GPU` / adapter log lines, frame pacing while animating a list, and behaviour after suspend and device loss.
