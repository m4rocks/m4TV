# Run the Engine as castLabs Electron, off-screen, in a child process of the Shell

The Engine is the castLabs build of Electron (ECS), pinned exactly (`44.5.1+wvcus`), run off-screen as a child process that the Shell spawns and supervises. It replaces the embedded CEF of ADR 0001, because official CEF lacks H.264 and AAC, no one publishes a linuxarm64 CEF that has them, and the codecs are compiled into `libcef.so`, so the only way to get them was to compile Chromium ourselves. Electron ships its FFmpeg with the proprietary codecs, and castLabs publishes a prebuilt stable linux-arm64 build with Widevine, so nothing is compiled. The reasons for ADR 0001 still hold: the Shell draws Overlays on a playing App, so the Engine renders off-screen and the Shell shows its frames.

## How it works

- The Shell starts Electron as a child, restarts it if it dies, and it never outlives the Shell. A host script in the repo creates one hidden off-screen window per App, each in its own persistent storage partition, and injects the spatial-navigation script and the TV tuning.
- The Shell and the host talk over a Unix socket in the runtime directory: launch, key, text and focus commands in one direction, "frame ready" and live state in the other.
- Frames: the host copies each paint bitmap into a `/dev/shm` file and says so over the socket, and the Shell reads it into a gpui image. This is pure JavaScript, with no native addon. A dmabuf shared texture through a Rust addon is the next step only if the Pi 4 spike (#32) misses its budget, and gpui cannot import a dmabuf, so it would still copy until a gpui fork exists.
- The Dev host runs the same castLabs build and host script on Windows. The `cef` crate, its download and the cmake/ninja build step are removed.
- The image build downloads the castLabs zip from its GitHub release, checked against a tag and sha256 pinned in the repo. We build and host nothing; if castLabs removes a release we mirror that one zip.
- The Widevine CDM is still fetched on the device at first launch (castLabs's component updater) and never baked into the image.

## Considered Options

- **Stay on CEF and build it ourselves** (ADR 0001): no rework, and the build would run on a rented cloud VM as a one-button job. Rejected because it means compiling Chromium for linuxarm64 on every CEF bump, which the maintainer does not want to own.
- **Stay on CEF with stock builds first**, building only if an App fails: cheap to try, but Spotify's AAC and Netflix's H.264 likely need the codecs, and it only postponed the choice.
- **Qt WebEngine, WPE or Raspberry Pi's Chromium**: rejected earlier for Widevine support and for the Chromium-in-its-own-window problem that blocks Overlays (ADR 0001).
- **Ship without H.264/AAC**: rejected because it would lose Spotify and part of Netflix.

## Consequences

- castLabs states no redistribution terms for its binaries. The patent question for H.264/AAC stays an accepted risk (the distribution posture, #14). Release packaging records Electron's and Chromium's licences, including the LGPL FFmpeg.
- Electron adds a Node process and a Chromium renderer, so memory on a 4 GB Pi 4 is higher than for a library embedded in the Shell. There is no memory bar in v1.
- The Engine is a separate process, so a crash is survivable but every frame crosses a process boundary: at least one more copy than ADR 0001's CPU path. The Pi 4 frame rate is still unproven (#32).
- Widevine on Windows needs VMP-signed builds, so DRM Apps are only provable on the Pi. The Dev host shows the look and the behaviour, never DRM or performance.
- castLabs updates "on a best-effort basis". We bump the pin deliberately and re-check the Apps.
- Electron running hidden under Cage, and whether it composites on the GPU there, are unproven and are the first Pi check (#30).
- The key-to-virtual-key mapping and manual disposal of old frames from ADR 0001 stay, now on the host side.
