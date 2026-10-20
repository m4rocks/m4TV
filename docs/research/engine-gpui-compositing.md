# How can an Engine surface be composited with a gpui Shell on Wayland (Cage, Pi 4 / Pi 5)?

Research for GitHub issue m4rocks/m4TV#8. Date of research: 2026-10-20.

Question: how can a CEF or WPE (or other) Engine surface be composited together with gpui UI on Wayland: Wayland subsurfaces, dmabuf sharing, offscreen rendering into a gpui texture, or separate windows under a kiosk compositor (Cage)? Cover input routing, overlays (Shell on top of an App) and performance cost. Context: [widevine-engines-pi.md](widevine-engines-pi.md) (CEF is the likely Engine) and [gpui-on-pi.md](gpui-on-pi.md) (Shell uses crates.io `gpui-pre*` 0.3.9).

Nothing here was run on a Pi, or on any GPU. Everything is read from source, headers and issue threads. Performance numbers are arithmetic, not measurements.

## Pinned sources

Citations use these short names. "path:line" is relative to the source named.

| Short name | What | Pin |
|---|---|---|
| `gpui-pre`, `gpui-pre-linux`, `gpui-pre-wgpu`, `gpui-pre-platform` | crates.io tarballs, version 0.3.9, "snapshot of zed@93d7456" (I read the crate tarballs, not Zed's repo) | crates.io `.../api/v1/crates/<name>/0.3.9/download` |
| `cef` | https://github.com/chromiumembedded/cef master | `a875f407e36bdf4fe6c31622e150ce73f77629c8` (2026-10-10); also branches 7977 (Chromium 152.0.7977.167, current stable `linuxarm64`) and 8078 (156.0.8078.17, beta `linuxarm64`) |
| `cage` | https://github.com/cage-kiosk/cage | `ef6ef6a54b646db1dd468e69f3948765838a3c5b` (2026-09-08), tag v0.3.1 is latest release |
| `wlroots` | https://gitlab.freedesktop.org/wlroots/wlroots branch 0.20 (the version Cage's README says it is based on) | `d783533489e1f75d6886c2ab5c5960090ef268f8` |
| `wayland.xml` | https://gitlab.freedesktop.org/wayland/wayland `protocol/wayland.xml` | last change to the file `93c04ad3670633aea6f2435059378b75843b8e70` (I fetched `main`; the quoted text is from the interfaces named) |
| `chromium` | https://chromium.googlesource.com/chromium/src | tag `156.0.8078.0` |
| `wgpu-hal` | crates.io tarballs 29.0.4 and 30.0.1 | versions as named |
| `mesa` | https://gitlab.freedesktop.org/mesa/mesa `src/broadcom/vulkan/v3dv_device.c` | fetched from `main` at `fb891a59a94b8791b40ad2ef6b6ae153b882da88` (2026-10-20) |
| `wpe` | https://github.com/WebKit/WebKit `Source/WebKit/WPEPlatform` | `f88678d334a79ca75257c708a188c9280eecde3a` (2026-10-20) |

## Short answer

1. **There is no zero-copy path from a CEF frame into a gpui window today.** CEF's own contract for shared textures says the handle "cannot be cached and cannot be accessed outside of this callback" and its contents "should be copied to a texture owned by the client application" (`cef` include/cef_render_handler.h:151-167). Every in-process GPU path therefore costs at least one GPU blit per frame, and gpui cannot import a dmabuf at all (see section 2).
2. **A Wayland subsurface cannot cross processes.** Chromium's browser process is the sole Wayland client of a CEF app and makes its own connection (`chromium` ui/ozone/platform/wayland/README.md; `chromium` ui/ozone/platform/wayland/host/wayland_connection.cc:208). A `wl_subsurface` must be created from two `wl_surface` objects (`wayland.xml` `wl_subcompositor.get_subsurface`). CEF therefore cannot be made a subsurface of gpui's window. The only subsurface design is the Shell process creating its own subsurface and attaching frames it received from CEF OSR.
3. **The only approach that works with unmodified `gpui-pre` 0.3.9 is separate windows** (CEF windowed with Ozone/Wayland, gpui as a second toplevel) **or CPU OSR into `RenderImage`.** Separate windows under Cage work for the Engine, but Cage has no layer-shell, no xdg-activation and no client-side way to raise a window, so "Shell overlay on top of a playing App" and "Shell takes the keyboard back" both need workarounds (section 4).
4. **CEF supports Ozone/Wayland and shared-texture OSR with dmabuf planes on Linux**, but the Wayland shared-texture demo and a driver-compatibility fix only landed in October 2026 on master / 156 beta, tested on a VMware VM with Mesa, not on V3D. The 152 stable `linuxarm64` build has the API but not those changes (section 3).
5. **Nothing was measured on a Pi.** The CPU OSR path is at least 3 full-frame CPU copies per frame by my reading of the code; at 1080p that is about 8.3 MB per copy per frame (arithmetic in section 6).

| Approach | Needs gpui fork? | Input routing | Shell over App | Per-frame cost (1080p, unmeasured) | Main blocker |
|---|---|---|---|---|---|
| A. Two toplevels under Cage (CEF windowed, gpui separate window) | No | Compositor focus rules; Shell cannot force focus | Transparent gpui toplevel stacked above; unverified under Cage | None in app code; compositor must composite two buffers (no direct scanout) | Cage cannot raise or focus a client window without foreign-toplevel; Cage doc says one visible app |
| B. CEF OSR software (`OnPaint`) into gpui `RenderImage` | No | Shell owns all input, forwards to CEF | Natural (one window, one z-order) | >= 3 CPU full-frame copies + GPU upload | CPU/bandwidth on Pi; no partial upload; atlas churn |
| C. CEF OSR shared texture (dmabuf) into gpui texture | **Yes** (gpui-pre-wgpu has no import; wgpu 29 has no dmabuf import) | Same as B | Natural | 1 GPU blit (+ import), no CPU copy | Fork gpui-pre-wgpu, probably bump to wgpu 30; unproven on V3D |
| D. Shell-owned `wl_subsurface` fed by CEF OSR frames | No fork of the renderer, but raw Wayland code | Shell must read pointer on its own `wl_pointer`; subsurface never gets keyboard focus | Subsurface can be placed above or below the gpui surface | Buffer attach per frame, can be dmabuf (no copy) | gpui ignores events for surfaces it does not own; hand-written Wayland glue; prototype needed |
| E. WPE with an embedder-owned display | Not applicable | Embedder feeds events | n/a | n/a | Widevine blocker already established in widevine-engines-pi.md |

## 1. What gpui-pre 0.3.9 gives you

### 1.1 What the Wayland client binds

- Required globals: `wl_compositor`, `wl_shm`, `xdg_wm_base` (`gpui-pre-linux` src/linux/wayland/client.rs:283, 295, 301). Optional: viewporter, fractional scale, decorations, layer shell, blur, text-input v3, cursor shape, activation, data device, primary selection (client.rs:275-309).
- `wl_subcompositor` is **not** bound, and `zwp_linux_dmabuf_v1` is only used in a short-lived probe to read the compositor's main DRM device (client.rs:1524-1583). gpui itself never creates a subsurface and never imports a dmabuf from Wayland.

### 1.2 The `Surface` element is macOS-only

- `SurfaceSource` has one variant, a CoreVideo `CVPixelBuffer`, behind `cfg(macos|ios)` (`gpui-pre` src/elements/surface.rs:10-16, 35). `PaintSurface` carries `image_buffer: CFRetained<CVPixelBuffer>` on those targets only (src/scene.rs:768-774). `Window::paint_surface` is `cfg(macos|ios)` (src/window.rs:5155).
- The wgpu renderer drops these primitives: "Surfaces are macOS-only for video playback and are not implemented by the WGPU renderer." (`gpui-pre-wgpu` src/wgpu_renderer.rs:1612-1614)

### 1.3 No access to the wgpu device from app code

- `WgpuContext` has `pub device` and `pub queue` (`gpui-pre-wgpu` src/wgpu_context.rs:11-15), but I found no accessor on `Window`, `App` or the platform traits that hands it to application code. A search of `gpui-pre`, `gpui-pre-platform` and `gpui-pre-linux` for `WgpuContext`/`.device()`/`.queue()` found only internal use (renderer creation in `gpui-pre-linux` src/linux/wayland/window.rs:563, 834).
- `canvas(prepaint, paint)` only exposes "the low level paint API" (`gpui-pre` src/elements/canvas.rs:8-22): it adds scene primitives (quad, path, sprites, ...; `Primitive` enum in src/scene.rs:225-231). There is no "custom GPU pass" primitive.
- Conclusion: importing a dmabuf into a texture that gpui samples is **fork-only** (inference from the absence of any hook; the fork point is `gpui-pre-wgpu`).

### 1.4 wgpu version and dmabuf import

- `gpui-pre-wgpu` depends on `wgpu` 29.0 (Cargo.toml `[target...dependencies.wgpu] version = "29.0"`; features include `vulkan` and `gles`).
- `wgpu-hal` 29.0.4 `src/vulkan/` only enables the `VK_EXT_external_memory_dma_buf` device extension when supported (adapter.rs:1302-1303). It has **no** dmabuf import function (grep of `src/vulkan/*.rs` for `dmabuf`/`dma_buf` finds only adapter.rs).
- `wgpu-hal` 30.0.1 adds the `VULKAN_EXTERNAL_MEMORY_DMA_BUF` feature (adapter.rs:1062) and `Device::texture_from_dmabuf_fd(fd, desc, drm_modifier, stride, offset)` (src/vulkan/device.rs:525). Its docs say "Currently only supports single-plane DMA-bufs" (rendered at docs.rs/wgpu-hal/30.0.1, `vulkan::Device`) and that Vulkan takes ownership of the fd on success and closes it on failure; so CEF's fd must be `dup`ed first.
- Mesa's v3dv driver (Pi 4 and 5 Vulkan) advertises `EXT_external_memory_dma_buf` and `EXT_image_drm_format_modifier` (`mesa` src/broadcom/vulkan/v3dv_device.c:259, 264), so the driver side exists.
- Consequence: option C needs a `gpui-pre-wgpu` fork on wgpu >= 30 (or raw `ash` + `texture_from_raw` on 29, whose signature takes a `TextureMemory::External` per docs.rs/wgpu-hal/29.0.4). gpui's wgpu instance picks Vulkan first and falls back to GL (`gpui-on-pi.md` section 1.1), so the import also needs a GL/EGL variant if Vulkan fails. I did not verify the wgpu 30 move is compatible with `gpui-pre`'s other code.

### 1.5 The no-fork path: `RenderImage` through the sprite atlas

- `RenderImage` is "a cached and processed image, in BGRA format" with a fresh `ImageId` per `RenderImage::new` (`gpui-pre` src/assets.rs:42-61).
- `Window::paint_image` inserts it into the sprite atlas keyed by that id (src/window.rs:5053-5090); `drop_image` removes it (src/window.rs:5175-5186). **A caller that makes a new `RenderImage` per CEF frame must call `drop_image` on the previous one, or the atlas grows.**
- The atlas upload makes a full copy: `swizzle_upload_data` does `bytes.to_vec()` (and a B/R swap if the surface format is RGBA) before queuing the upload (`gpui-pre-wgpu` src/wgpu_atlas.rs:256-262, 381-392); then `queue.write_texture` uploads the whole image (src/wgpu_atlas.rs:265-295). There is no dirty-rect upload API.
- Atlas textures are capped by the device's `max_texture_dimension_2d` (src/wgpu_atlas.rs:53, 192-198). v3dv caps `maxImageDimension2D` at the framebuffer limit (comment: "we still cap maxImageDimension* at the framebuffer size", `mesa` v3dv_device.c:995-1006; the numeric value `max_fb_size` is defined above that and I did not read it). The wgpu atlas therefore bounds the largest single image; 4K frames may not fit one atlas texture. Not verified.

### 1.6 Input in gpui

- `Window: HasWindowHandle + HasDisplayHandle` (`gpui-pre` src/window.rs:7572-7587); on Wayland these return the `wl_surface` and `wl_display` pointers (`gpui-pre-linux` src/linux/wayland/window.rs:1686-1710). `wayland-backend` is built with `client_system` (Cargo.toml), so app code can wrap the foreign display (inference, not prototyped).
- gpui only handles pointer and keyboard events for surfaces it owns: `get_window` looks the surface up in `state.windows` and the event is ignored if absent (client.rs:1734-1739, 2088-2089, 2402). Events for a subsurface the app made are not delivered to the Shell's gpui code; the app would need its own `wl_pointer` (inference).
- `Keystroke` has only `modifiers`, `key` (string) and `key_char` (`gpui-pre` src/platform/keystroke.rs:18-33). The raw xkb keycode exists inside the Wayland client (client.rs:2162-2168) but is not in the event. CEF's `CefKeyEvent` wants `windows_key_code` (a DOM/Windows virtual key code) and `native_key_code` plus `character` and `unmodified_character` (`cef` include/internal/cef_types.h:2401-2447). The Shell therefore needs its own key-name to Windows-VK table for OSR (B, C, D). IME text from gpui (it binds text-input v3) maps to `CefBrowserHost::ImeCommitText` (`cef` include/cef_browser.h:865).
- `Window::set_input_region(Option<&[Bounds]>)` exists and forwards to `wl_surface.set_input_region` (`gpui-pre` src/window.rs:2518; `gpui-pre-linux` src/linux/wayland/window.rs:2134-2160). Per the protocol, "Input events happening outside of this region will try the next surface in the server surface stack" (`wayland.xml` `wl_surface.set_input_region`).
- Window kinds: `Normal`, `PopUp`, `AnchoredPopup`, `Floating`, `Dialog`, `LayerShell` (`gpui-pre` src/platform.rs:2706-2733). `Floating`/`Dialog` call `xdg_toplevel.set_parent` (`gpui-pre-linux` window.rs:263-264). `LayerShell` returns `LayerShellNotSupportedError` if the compositor lacks it (window.rs:162-164).
- Transparency: the Wayland window can be created transparent, the surface alpha mode prefers `PreMultiplied` (`gpui-pre-wgpu` src/wgpu_renderer.rs:415-428), and it only sets an opaque region when the background is `Opaque` (`gpui-pre-linux` window.rs:2311-2317).

## 2. What CEF gives you on Linux

### 2.1 Ozone/Wayland

- On Linux Chromium builds both X11 and Wayland Ozone backends by default (`chromium` build/config/ozone.gni, `is_linux` branch: `ozone_platform = "x11"`, `ozone_platform_wayland = true`, `ozone_platform_x11 = true`; docs/ozone_overview.md: "By default, Linux enables the following Ozone backends - X11, Wayland and Headless"). The **default runtime platform is x11** (ozone.gni) unless overridden.
- CEF's own tests detect Wayland by `--ozone-platform=wayland` or `XDG_SESSION_TYPE=wayland` ("Same default as ui::SetOzonePlatformForLinuxIfNeeded()", `cef` tests/shared/browser/util_linux.cc:14-22). Cage sets `WAYLAND_DISPLAY` (cage.c:677) but I did not find it setting `XDG_SESSION_TYPE`. **Pass `--ozone-platform=wayland` explicitly** (my inference; not tested under Cage).
- CEF's `CefUIThread` has Wayland-specific handling for multi-threaded message loop (`cef` libcef/browser/ui_thread.cc:79-80).
- Chromium's Wayland host binds `wl_compositor`, `wl_subcompositor`, `xdg_wm_base`, `wl_shm`, and uses `zwp_linux_dmabuf` for GPU buffers (`chromium` ui/ozone/platform/wayland/host/wayland_connection.cc:276, 636-661; README.md "CreateDmabufBasedBuffer()"). Cage provides all of these (cage.c:433 `wlr_compositor`, 440 subcompositor, 398 `linux_dmabuf_v1` v4, 505 xdg shell v5; `wl_shm` comes with the wlroots renderer, which I did not trace).
- **Windowed CEF with a native parent window does not work on Wayland**: "Windowed browsers are parented to an X11 window, and CEF does not support parent windows with the Wayland Ozone platform." (`cef` tests/cefclient/cefclient_gtk.cc:91-98). The supported Wayland windowed mode is the Views API (cefclient `--use-views`; the check above only blocks the non-Views, non-OSR path). So you cannot reparent a CEF window into a gpui window; the Engine is always its own toplevel (approach A) or an offscreen browser (B, C, D).

### 2.2 OSR output: software and shared texture

- `CefRenderHandler::OnPaint` gives a BGRA buffer, upper-left origin, `width*height*4` bytes (`cef` include/cef_render_handler.h:129-149), used when `CefWindowInfo::shared_texture_enabled` is false. Default frame cap is 30 fps, minimum 1, settable via `windowless_frame_rate` or `SetWindowlessFrameRate` (include/internal/cef_types.h:602-610; cef_browser.h:812-821).
- `OnAcceleratedPaint` is used with `shared_texture_enabled`. On Linux "it contains several planes, each with an fd to the underlying system native buffer"; the handle is pooled and "cannot be cached and cannot be accessed outside of this callback. It should be reopened each time ... and the contents should be copied to a texture owned by the client application. The contents of |info| will be released back to the pool after this callback returns." (include/cef_render_handler.h:151-168).
- The Linux struct carries up to 4 planes (`stride`, `offset`, `size`, `fd`), `plane_count`, `modifier`, `format` (BGRA or RGBA) and `extra` (coded size, visible rect, damage) (include/internal/cef_types_linux.h:150-212; filled from the GPU buffer's `native_pixmap_handle` at libcef/browser/osr/video_consumer_osr.cc:186-213).
- `external_begin_frame_enabled` plus `SendExternalBeginFrame()` lets the embedder drive frame production (include/cef_browser.h:740-745).
- The docs page still says "Off-screen rendering does not currently support accelerated compositing" (`cef` docs/general_usage.md, Off-Screen Rendering section). That text contradicts the header above and appears stale; I trust the header and source.

### 2.3 Version gating and known requirements for Linux shared-texture OSR

- Linux `OnAcceleratedPaint` plumbing exists in both branch 7977 (stable 152, the current `linuxarm64` stable) and 8078 (beta 156): `video_consumer_osr.cc` has the `IS_LINUX` native-pixmap branch in both (branch 7977 line 186).
- **Not in 152 stable:** `osr_gl_linux.cc` (the cefclient dmabuf-import renderer and GTK Wayland OSR path) is absent on branch 7977 (GitHub contents API returned 404) and present on 8078. It was added by commit `c7e1a9e5c` (2026-10-09, "osr: linux: cefclient: Add OnAcceleratedPaint support (fixes #3687)"), which says it was "Tested on a Linux VM (VMware SVGA3D, Mesa 23.2, GNOME Wayland/Xwayland)" including cefclient shared-texture on Wayland via GtkGLArea. The commit message does not mention any ARM device or V3D.
- **Not in 152 stable:** commit `c5b06184b` (2026-10-09, issue #4237) switches the capturer from `kPreferMappableSharedImage` to `kPreferSharedImageWithNativeHandle` because the former "forces Linux into SCANOUT_CPU_READ_WRITE / GBM_BO_USE_LINEAR, which breaks GPU rendering on NVIDIA". Branch 7977 still uses `kPreferMappableSharedImage` (video_consumer_osr.cc:54). The reported failure is NVIDIA-specific; whether it affects Mesa V3D is unknown.
- Known requirements: cefclient adds `--use-angle=gl-egl` when both `off-screen-rendering-enabled` and `shared-texture-enabled` are set, with the reason "Without this, DMABUF based rendering will fail" (`cef` tests/shared/browser/client_app_browser.cc:76-90; issue #3953). Issue #3953 also states the GPU process import path needs `GetGLOzone()` for the current Ozone platform (X11 or Wayland). Issue #4237 lists `--use-angle=gl-egl` and `--ozone-platform=x11` as "required for Linux shared-texture OSR" at the time of that report; the October commit adds a Wayland test, so the X11 requirement may be out of date.
- CEF ships a reference importer: EGL `eglCreateImageKHR` with `EGL_LINUX_DMA_BUF_EXT` + `glEGLImageTargetTexture2DOES`, with format fourcc ARGB8888/ABGR8888 and a `SupportsDmaBufImport(display)` check (`cef` tests/shared/browser/osr_gl_linux.cc:63-91, 144; osr_renderer_gl_linux.cc:499-540). It makes a new EGL image per callback.
- Windowless rendering "requires Alloy style" (cefclient comment, tests/cefclient/browser/browser_window_osr_gtk.cc:1143), and CEF warns `windowless_rendering_enabled` "may reduce rendering performance on some systems" if enabled when unused (include/internal/cef_types.h:268-274).

### 2.4 CEF input in OSR

Embedder sends `SendKeyEvent`, `SendMouseClickEvent`, `SendMouseMoveEvent`, `SendMouseWheelEvent`, `SendTouchEvent`, `SetFocus`, `SendCaptureLostEvent` and IME calls (`cef` include/cef_browser.h:401, 751-794, 865). With OSR the Shell is the only Wayland client for input, so there is one keyboard focus and one pointer focus. Popups (select boxes) arrive as a second paint element (`OnPopupShow`/`OnPopupSize`, include/cef_render_handler.h:112-129) that the Shell must composite itself.

## 3. Option details

### A. Two toplevels under Cage

How: Cage launches the Shell as its primary client; the Shell spawns the CEF process (Views window, `--ozone-platform=wayland`), which maps a second toplevel.

What the sources say about Cage:

- Cage "runs a single, maximized application ... can run multiple applications, but only a single one is visible at any point in time" (`cage` cage.1.scd, DESCRIPTION). The code does not hide lower views: `view_map` creates a scene tree on top of all other nodes (new scene nodes are appended last = on top: `wlroots` types/scene/wlr_scene.c:81-83), positions it (maximized if it has no xdg parent, centered if it has one and fits: `cage` view.c:94-110, xdg_shell.c:139-145) and focuses it (view.c:178). wlroots renders every visible buffer, so a transparent top window would show the lower one. **That the manual and the code disagree is why I list transparent-over-opaque under Cage as unverified.**
- **Cage ends when the primary client exits**: it forks the app from the command line and terminates on SIGCHLD (`cage` cage.c:157-193, 689). The Shell must therefore be the primary client and own the Engine child process.
- **No layer-shell.** `grep -i layer` over every Cage source file returns nothing. gpui's `WindowKind::LayerShell` therefore fails with `LayerShellNotSupportedError` under Cage (`gpui-pre-linux` window.rs:162-164).
- **No xdg-activation** in Cage either (not created in cage.c). Cage does create `wlr_foreign_toplevel_manager_v1` (cage.c:626); its `request_activate` handler raises the view to the top and focuses it (`cage` view.c:131-138; request emitted in `wlroots` types/wlr_foreign_toplevel_management_v1.c:115-119). That is the **only** client-initiated way to raise/focus a window, and it needs a client of `zwlr_foreign_toplevel_manager_v1`, which gpui does not implement (`gpui-pre-linux` binds no foreign-toplevel; grep found none). The Shell would need its own small protocol client on the shared display, or the Shell window must be destroyed and recreated to get on top again (it then maps newest, hence topmost and focused). Both are inference from reading the code.
- **Focus on click.** A pointer press focuses the view under the cursor unless the currently focused view is a transient child of the clicked one (`cage` seat.c:97-111). The hit test is `wlr_scene_node_at`, which honours each surface's input region (`wlroots` types/scene/surface.c:381-389, wlr_scene.c:1401-1402; `wlr_surface_point_accepts_input` in types/wlr_compositor.c:997-1003). So a Shell toplevel with an empty input region is click-through and the Engine gets pointer events and focus; a Shell toplevel with a non-empty region over a button area gets them there.
- **Keyboard follows focus, not pointer.** `seat_set_focus` enters the keyboard to one view only (seat.c:940-970). With A, a TV remote (keyboard-like events) goes to whichever toplevel is focused. If the Engine is focused, the Shell does not see "Home"/"Menu" keys. Fix options: forward from the Engine (not possible in windowed CEF without CEF key hooks), make the Shell the keyboard owner via foreign-toplevel activate, or use B/C/D where the Shell owns all input.
- **Child toplevels:** `xdg_toplevel.set_parent` makes the child non-primary and centered rather than maximized; the focus-steal rule above also treats the parent/child pair specially (`cage` xdg_shell.c:139-158, seat.c:97-111). gpui's `Floating`/`Dialog` kinds set a parent. I did not test whether a full-screen `Floating` Shell window behaves as an overlay in Cage.

Cost: no pixel copies in application code. Chromium's GPU process hands dmabuf-backed `wl_buffer`s to Cage via the browser process (README flow above). **Compositing cost moves to Cage:** wlroots only scans out a buffer directly when the render list has a single entry ("There is only one entry in the render list", `wlroots` types/scene/wlr_scene.c:2441-2448, direct-scanout checks at 2061-2110), and `wlr_scene.c` has no use of KMS overlay planes (grep for `output_layer` finds none there; only `examples/output-layers.c`). So **any visible Shell overlay over the Engine forces full GPU composition of both buffers every frame**, using Cage's renderer (`wlr_renderer_autocreate`, cage.c:370). Cost on V3D not measured.

### B. CEF OSR (software) into a gpui `RenderImage`

How: windowless browser, `OnPaint` BGRA, wrap into `RenderImage`, `img()`/`paint_image` into the Shell's frame, `drop_image` the previous frame. Shell owns every input event and calls CEF `Send*Event`.

- One window, one z-order: overlays (menus, toasts, a progress bar over the video) are ordinary gpui elements above the image element.
- Copies per frame (my count from the code above; none measured): Chromium's own readback into the `OnPaint` buffer (unknown count inside Chromium), then the Shell's copy into a gpui image buffer, then `to_vec` in the atlas, then `write_texture` (whatever wgpu does internally). That is **at least 3 full-frame CPU copies in code I read**, plus a full-image GPU upload; no dirty-rect upload.
- CEF's default OSR frame cap is 30 fps (section 2.2). 60 fps needs `SetWindowlessFrameRate`.
- Needs the Engine's software-compositing path: no GPU shared images involved, so the 7977 shared-texture caveats do not apply, but `windowless_rendering_enabled` has its own cost warning (section 2.3).
- Works with `gpui-pre` 0.3.9 unmodified and with the stable CEF 152 `linuxarm64` build.

### C. CEF OSR shared texture imported into gpui's wgpu texture

How: `shared_texture_enabled=true` and `--use-angle=gl-egl` (and `--ozone-platform=wayland` or `x11`), in `OnAcceleratedPaint` `dup` the plane-0 fd, import with `texture_from_dmabuf_fd` (wgpu-hal 30, single plane only), `copy_texture_to_texture` into a Shell-owned texture, return from the callback. Draw it from a gpui render pass that needs a new primitive.

- Requires a fork of `gpui-pre-wgpu` (and a texture primitive in `gpui-pre` scene/window code, since `canvas` cannot issue GPU commands), a wgpu bump to >= 30, a GL fallback, and CEF >= 156 beta for the Linux demo/fixes (section 2.3).
- CEF reports the modifier (`info.modifier`); `texture_from_dmabuf_fd` takes `drm_modifier`, `stride` and `offset`, plane 0 only. If Chromium on V3D allocates multi-plane or an unsupported modifier, the import fails (unverified; V3D buffer layout was not examined).
- Cost: no CPU copy; one GPU-to-GPU blit plus import per frame. gpui's own wgpu queue and CEF's GPU process share the same V3D, so contention is possible; unmeasured.
- Unproven end-to-end on Pi.

### D. Shell-owned subsurface fed by CEF OSR

How: Shell creates a `wl_surface` on the same `wl_display` pointer gpui exposes, a `wl_subsurface` of gpui's window surface, attaches buffers made from CEF frames, and places it below or above gpui's surface with `place_below`/`place_above` (`wayland.xml` `wl_subsurface`).

- Protocol facts: a sub-surface "is initially in the synchronized mode"; position and z-order are "double-buffered state on the parent surface" and take effect on the parent's next commit; "a sub-surface never has the keyboard focus of any seat"; the aim of subsurfaces is to let the compositor "pass YUV video buffer processing to dedicated overlay hardware when possible" (`wayland.xml` `wl_subsurface`, `wl_subcompositor`). A transparent gpui region above a lower subsurface is the standard way to show video "behind" UI; gpui supports a transparent window (section 1.6).
- Cross-process is impossible (section "Short answer" 2), so the buffers must come from the Shell process: from CEF `OnPaint` (copy into a `wl_shm` buffer) or from `OnAcceleratedPaint` (import the dmabuf with `zwp_linux_dmabuf_v1` as a `wl_buffer`; the handle is only valid during the callback, so this still needs a copy or a held buffer; the contract says copy).
- gpui does not bind `wl_subcompositor` (section 1.1). The glue binds it on a separate event queue using `wayland-client` on the foreign display (inference, not prototyped). After changing the subsurface's position/order, gpui must commit its parent surface for the change to apply; the Shell must trigger a redraw (inference from the protocol text).
- Input: the subsurface is a different `wl_surface`; gpui ignores pointer events for surfaces it does not own (section 1.6). The glue needs its own `wl_pointer` on the same seat to read pointer events over the video; keys always go to the gpui toplevel (subsurface cannot be keyboard-focused), so gpui forwards keys to CEF (needs the key mapping table).
- Cost: with dmabuf `wl_buffer`s the compositor can composite without a CPU copy, but **Cage's wlroots scene does not use KMS overlay planes** (section A), so the "overlay hardware" benefit named in the protocol text does not materialise under Cage.
- No benefit over C compared to a gpui-internal texture unless you need compositor-side scaling or planes; it is the most hand-written option. I would not choose it.

### E. WPE and other engines

- `WPEPlatform` has `drm`, `headless` and `wayland` display implementations in tree (`wpe` Source/WebKit/WPEPlatform/{drm,headless,wayland}). `WPEDisplay` and `WPEView` are derivable GObject classes with a `create_view` display vfunc and a `render_buffer(view, buffer, damage_rects, n, error)` view vfunc; buffers are `WPEBufferDMABuf` (fd, format, modifier) or `WPEBufferSHM`; the view reports `wpe_view_buffer_rendered`/`wpe_view_buffer_released`; input is fed with `wpe_view_event` and `wpe_view_focus_in/out` (`wpe` WPEDisplay.h:48-67, WPEView.h:52-70, 128-148; WPEBufferDMABuf.h:42-59). In principle an embedder can implement its own display/view and receive frames, the equivalent of C. I did not build or run it.
- The Widevine problem for WPE is covered in widevine-engines-pi.md and is unchanged; this research does not make WPE a candidate.
- Not researched: a nested Wayland compositor inside the Shell (the Shell would act as the compositor for Engine toplevels and render their buffers itself, e.g. via smithay or wlroots bindings). This is the general form of A with full control of focus and stacking; it is a large piece of work and I did not look for primary sources on it.

## 4. Overlays: Shell on top of an App

| Approach | How the Shell shows UI over a playing App | Notes |
|---|---|---|
| A | Second toplevel above the Engine's toplevel | Newest mapped toplevel is on top and focused (`cage` view.c:178, `wlroots` wlr_scene.c:81-83). To come back to the top later: recreate the window, or use foreign-toplevel `request_activate`. Transparent-over-opaque not verified under Cage. |
| B, C | One gpui window; engine image is an element, UI elements above it | Z-order and focus entirely under Shell control |
| D | Subsurface below (or above) gpui surface; transparent gpui hole | Position/order changes apply on gpui's next parent commit |

The recommendation depends on one requirement. **If "Shell overlay on top of a playing App" with Shell-owned keyboard focus is mandatory, use B or C** (one window, Shell owns input). **If the Shell can instead hide/stop the App while the Shell is visible** (full-screen switch between Shell and App), A is simplest and the least CPU, using only a way to switch which toplevel is topmost/focused.

## 5. Input routing summary

| Approach | Keyboard (TV remote) | Pointer / touch |
|---|---|---|
| A | Whoever Cage focused (newest mapped, or clicked). Shell cannot force it without foreign-toplevel | Hit-tested by Cage through input regions; empty gpui input region makes the Shell click-through |
| B, C | Shell receives all keys, forwards via `SendKeyEvent` (needs key table; `Keystroke` has no keycode) | Shell receives, forwards via `SendMouse*Event` / `SendTouchEvent` |
| D | Shell toplevel receives keys (subsurface cannot be focused); forward to CEF | Needs a second `wl_pointer`; gpui ignores events for foreign surfaces |

## 6. Performance arithmetic (not measurements)

- One 1080p BGRA frame = 1920 x 1080 x 4 = 8,294,400 bytes. At CEF's default 30 fps that is about 248.8 MB/s per full-frame copy stage; at 60 fps about 497.7 MB/s per stage. A 4K frame is 4x as much.
- B has at least 3 such CPU stages in code I read (section 3.B), so about 750 MB/s of copying at 1080p30 before the GPU upload and Chromium's own readback. I did not find a source for Pi 4 or Pi 5 memory bandwidth and do not quote one.
- A and C avoid per-frame CPU copies. Costs are respectively compositor GPU composition of two buffers (no direct scanout) and one GPU blit.
- Idle and playing CPU/RAM of Chromium itself is already listed as unmeasured in widevine-engines-pi.md. Software decode on Pi 5 and H.264-only hardware decode on Pi 4 (same document) dominate any compositing cost for video; this research did not examine whether video frames from Chromium's hardware decoder reach OSR without readback.

## Unverified, or inferred without a test

- Whether a transparent gpui toplevel stacked above an opaque CEF toplevel shows both under Cage (code says yes; the man page says one app is visible).
- Whether Cage sets `XDG_SESSION_TYPE` (I found no code that does; Chromium's default platform is x11 per ozone.gni).
- Whether CEF 152 or 156 runs shared-texture OSR on V3D (Pi 4/5) at all; the only Wayland test in the sources is a VMware/Mesa VM. Not checked: V3D dmabuf modifiers, multi-plane output, `--use-angle=gl-egl` stability on Pi.
- Whether the wgpu 29 to 30 bump is feasible inside `gpui-pre-wgpu`; whether a Vulkan-only `texture_from_dmabuf_fd` path is enough given gpui's GL fallback.
- Maximum single atlas image size on V3D (`max_fb_size` value not read), so whether a 4K `RenderImage` fits.
- Subsurface glue (D): feasibility is inferred from the raw handles, the `client_system` backend and protocol text; no code was written. Whether a second `wl_pointer` on the same seat works alongside gpui's was not checked.
- Whether Pi's Cage package is v0.3.1 on wlroots 0.20; I read the latest Cage source, not a Pi OS package.
- Nested compositor approach, WPE custom display in practice, and Weston kiosk-shell or other compositors with layer-shell: not researched.
- The `docs/general_usage.md` statement that OSR lacks accelerated compositing looks stale; I did not trace its history.
- I did not review `gpui-pre`'s `img()` element path beyond `Window::paint_image`; any image-cache layer above `paint_image` was not read.

## Reproduction

```
# gpui-pre* 0.3.9 sources
curl -sL -A m4tv https://crates.io/api/v1/crates/gpui-pre-linux/0.3.9/download | tar xz   # likewise gpui-pre, gpui-pre-wgpu, gpui-pre-platform
# wgpu-hal import check
curl -sL -A m4tv https://crates.io/api/v1/crates/wgpu-hal/30.0.1/download | tar xz; grep -rn dmabuf src/vulkan
# CEF, Cage, wlroots
git clone --depth 1 https://github.com/chromiumembedded/cef
git clone --depth 1 https://github.com/cage-kiosk/cage
git clone --depth 1 -b 0.20 https://gitlab.freedesktop.org/wlroots/wlroots.git
gh api "repos/chromiumembedded/cef/contents/tests/shared/browser/osr_gl_linux.cc?ref=7977"   # 404
gh api repos/chromiumembedded/cef/commits/c7e1a9e5c ; gh api repos/chromiumembedded/cef/commits/c5b06184b
curl -s https://cef-builds.spotifycdn.com/index.json    # linuxarm64 versions
```
