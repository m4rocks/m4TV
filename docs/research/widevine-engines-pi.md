# Which non-Rust web engine plays Widevine content on Pi 4 and Pi 5?

Research for GitHub issue m4rocks/m4TV#3 (parent #2; blocks #9 and #8). Researched 2026-10-20.

Scope: non-Rust web engines that can be embedded into the Rust+gpui TV Shell on aarch64 Raspberry Pi 4 / Pi 5, and whether each can play Widevine-protected video. Primary sources only: upstream source code, vendor package repositories, first-party vendor help pages, upstream bug trackers. Where I inspected an artifact myself (downloaded a package, queried an API), the command is in the Reproduction section. Claims I could not tie to a primary source are listed in "Unverified" and are not stated as fact elsewhere.

## Short answer

1. **Widevine now exists for Linux arm64, officially.** Google's own apt repo ships `google-chrome-stable` for arm64 with a native `linux_arm64` `libwidevinecdm.so` (CDM 4.10.3112.0) [S3]. Before this, ARM64 Linux needed unofficial or 32-bit workarounds [S9]. The Chromium source tree lists `target_os == "linux" && target_cpu == "arm64"` as a platform where a library CDM is available [S1]; I confirmed the same line at tag 152.0.7977.167 (the Chromium version of the current CEF linuxarm64 build) as well as on `main`.
2. **The security level on every engine below is L3 (software only).** Desktop Linux Chromium registers only a `kSoftwareSecure` Widevine entry; the hardware-secure path exists only for ChromeOS and Android [S2]. I found no Widevine L1 path on Raspberry Pi hardware in any primary source (absence of evidence, not proof).
3. **CEF (Chromium) is the best fit** among embeddable engines: official `linuxarm64` builds exist [S10], CEF builds with `enable_widevine=true` [S5], and CEF fetches the CDM with Chromium's component updater, so you do not ship the CDM yourself [S5]. Caveats: official CEF builds are not built with proprietary codecs (no H.264), and I could not verify that the component updater serves a linux-arm64 CDM (see Unverified).
4. **Qt WebEngine** works the same way as CEF (Chromium plus a Widevine you supply) [S6]. **Electron (castLabs fork)** ships linux-arm64 builds with Widevine as of 2026-08, marked experimental [S8]. **Firefox** on Pi OS works with Google's CDM, but upstream Mozilla still lists linux-arm64 Widevine as an open bug [S9, S4].
5. **WPE WebKit is the weakest option for Widevine.** EME in WPE needs the Thunder/OpenCDM path, which is off by default and links Widevine's licensed CE-CDM libraries, not the `libwidevinecdm.so` that Chrome ships [S7a, S7b, S7c]. I found no primary source showing it working on Pi.
6. **Hardware decode is narrow.** Pi 4 has H.264 and HEVC decode blocks; Pi 5 has HEVC only, and "other codecs run in software" [S11]. Neither Pi kernel tree contains a VP9 or AV1 hardware decoder driver [S12]. Raspberry Pi's own Chromium build (155) wires up only Chromium's V4L2 *stateful* decoder and deliberately hides stateless devices, which includes the HEVC decoder [S20]. So the only realistic hardware path in a Chromium-based engine is H.264 on Pi 4 via `bcm2835-codec`; on Pi 5 plan on software decode of everything (H.264, VP9, AV1), and HEVC hardware needs custom integration work.
7. **Legal gate:** Google's Widevine licence file says you may not "use, modify, sell, or otherwise distribute the Software without a separate license agreement with Google" [S3]. For a distributed OS image, m4TV should have the device download the CDM at runtime (component updater) rather than bake it into the image.

## 1. How the CDM is obtained on ARM Linux

| Route | Primary evidence | Notes |
|---|---|---|
| Google Chrome arm64 `.deb` / apt repo | The arm64 `Packages` index lists `google-chrome-stable` 155.0.8059.84 (also beta 157, canary 158) [S3]. The `.deb` contains `/opt/google/chrome/WidevineCdm/_platform_specific/linux_arm64/libwidevinecdm.so` (20,041,928 bytes, ELF machine `0xb7` = AArch64) and a `manifest.json` with version 4.10.3112.0 listing `linux/arm64` [S3]. | Inspected by me. Google's `manifest.json` `x-cdm-codecs` is `vp8,vp09,avc1,av01` (no HEVC). `x-cdm-host-versions` is `10,11`. |
| Chromium component updater | Chromium registers a Linux CDM either from a bundled directory (only if `BUNDLE_WIDEVINE_CDM`, i.e. Chrome-branded) or from a "hinted" directory populated by the component updater (`ENABLE_WIDEVINE_CDM_COMPONENT`) [S2][S1]. Library path must be `<base>/_platform_specific/<os_arch>/libwidevinecdm.so`, manifest at `<base>/manifest.json` [S2]. | On Linux the CDM is loaded into the zygote at startup, so a newly downloaded CDM needs an app restart [S5]. |
| CEF | CEF issue #3149: CEF restored component-updater use; downloaded CDM goes under `CefSettings.user_data_path/WidevineCdm` (Linux default `~/.config/cef_user_data/WidevineCdm`). Maintainer also states Widevine "will load from disk if you create the correct directory structure before initializing CEF" [S5]. CEF patches Chromium so the Linux hint file lives under the user-data dir [S5]. | Quick test per CEF: shaka-player support page must show non-null `com.widevine.alpha` [S5]. |
| Qt WebEngine | "The Widevine plugin is not shipped with Qt WebEngine." Point to it with `QTWEBENGINE_CHROMIUM_FLAGS=--widevine-path=".../libwidevinecdm.so"` [S6]. | You must supply the file yourself (see licence note). |
| Raspberry Pi OS package | Pi's apt repo (trixie and bookworm, arm64) has `libwidevinecdm0` 4.10.2662.3+1, maintained by a Raspberry Pi engineer, installing to `/opt/WidevineCdm` with `linux_arm64` as a symlink to `gmp-widevinecdm/latest` [S4]. Firefox `Recommends` it [S4], and so does Pi's Chromium source package (`widevine_recommends := libwidevinecdm0`, changelog "Add package recommend for libwidevinecdm0") [S20]. | Older CDM than Chrome's (2662 vs 3112). Manifest codecs `vp8,vp9.0,avc1,av01`, host versions `10` only [S4]. The binary `chromium` 155 index entry I read showed Recommends of only `chromium-l10n` and `rpi-chromium-mods` [S4], but the newer source package (changelog dated 2026-10-16) adds the `libwidevinecdm0` recommend [S20]; the archive index may lag the source. Pi's `/opt` out-of-band CDM patch now applies only to 32-bit ARM (`ARCH_CPU_ARM_FAMILY && !ARCH_CPU_ARM64`; changelog "Restrict Widevine to armhf"); on arm64 Pi's Chromium takes Chromium's normal Linux paths (bundled dir next to the binary, or component-updater hint dir), and Pi leaves the Debian component-updater-disable patch commented out in `series` [S20]. I did not trace how the arm64 `/opt/WidevineCdm` package is found by Pi's Chromium; Debian's README says to place the CDM in `~/.config/chromium/WidevineCdm/` or `/usr/lib/chromium/WidevineCdm/` [S20]. |
| castLabs Electron | castLabs release notes: "Provides support for `linux-arm64` now that the Widevine is officially available for the platform" (v44.5.1+wvcus, 2026-10-03; first linux-arm64 note v44.0.0-alpha.9, 2026-08-06, called experimental) [S8]. | Third-party fork of Electron; CDM fetched by its own component updater. |
| Thunder/OpenCDM (WPE) | `OCDM-Widevine` CMake `find_package(WideVine)` and links `widevine_ce_cdm_static`, `oec_level3_static`, `widevine_cdm_core`, etc. [S7c]. | This is the Widevine CE-CDM SDK, built into a Thunder plugin. Not a drop-in for Chrome's `libwidevinecdm.so`. The 2020 Igalia post says Widevine "is closed source" and needs credentials to download [S7b]. |

CDM/host compatibility: a CEF maintainer relayed Google's guidance that the CDM is expected to work "as long as the CDM interface is unchanged (currently at 10)" and that Google tests only against current Chrome stable [S5]. Chrome's current arm64 CDM manifest advertises interface version 10 and host versions 10,11 [S3]. That was Google's 2021 statement; I did not find a newer one.

Page-size note (matters for Pi 5): the Pi 5 default kernel `kernel_2712.img` uses 16K pages [S13]. Chrome's arm64 CDM has all `PT_LOAD` segments aligned to 0x10000 (64K), so it is loadable on 16K pages [S3, my ELF parse]. A Raspberry Pi engineer reports in Mozilla's tracker: "in practice the official Google widevine library worked fine on my Pi5" [S9].

## 2. Security level reached and resulting caps

**Level.** Chromium on desktop Linux registers Widevine only with `Robustness::kSoftwareSecure`; `AddHardwareSecureWidevine()` has no Linux branch. The hardware-secure path is gated on `USE_CHROMEOS_PROTECTED_MEDIA` (ChromeOS) or is Android-only [S2]. In Widevine terms, software-secure is the L3 tier. Google's own Widevine documentation that defines L1/L3 is partner-gated ("Available documentation is contingent upon licensing Widevine") [S14], so the L1=TEE and L3=software mapping is the industry-standard reading but I did not get it from a public Google page (see Unverified). All engines in this report use the same Chromium code path (CEF, Qt, Electron), or Mozilla's equivalent (Firefox), or Widevine's `oec_level3` OEMCrypto (WPE/Thunder build) [S7c], so L3 applies to all of them.

**Per service**

| Service | What a primary source says | Implication for Pi |
|---|---|---|
| Netflix | Netflix Help (Linux row): Chrome 117+, Firefox 129+, Edge 118+, Opera 92+ are "capped at 1080p". Netflix also says "we can't guarantee Netflix will work on devices with a Linux OS" and its support will not troubleshoot Linux [N1][N2]. | Ceiling is 1080p on Linux browsers. No 4K or HDR (those need Windows/Mac Safari/ChromeOS paths on that same page). |
| Spotify | Spotify's web player requires Widevine ("Playback of protected content is not enabled" is fixed by enabling Widevine) [SP1]. Web player bitrates: Free AAC 128 kbit/s, Premium AAC 256 kbit/s [SP2]. Spotify's page does not mention Linux. | Audio only, so no resolution cap and L3 is sufficient. Works only if the engine exposes `com.widevine.alpha`. |
| YouTube | YouTube Help: purchased/rented titles depend on "studio licensing" for HD/UHD per device; the requirements page lists Ubuntu 10+ for movies and says HD streaming in a browser is "unavailable ... except for Safari" [Y1][Y2]. 4K purchased playback is listed for smart TVs, Android TV, Chromecast Ultra [Y1]. | Purchased/rented movies on a Pi browser should be treated as SD-only unless proven otherwise. The "HD unavailable in a browser except Safari" line appears on one regional version of that page and not on another, so treat it as unconfirmed; I found no other Linux-specific YouTube statement. Ordinary YouTube videos are outside this DRM question; see Unverified. |

Chrome itself (not embeddable in gpui, but useful as a reference) is the same L3 browser: Chromium source has no hardware-secure Linux path [S2].

## 3. Engines compared

| Engine | Runs on aarch64 Pi 4/5? | Widevine | Main limits |
|---|---|---|---|
| **CEF** (Chromium) | Official `linuxarm64` CEF builds published; latest at research time 152.0.14 / Chromium 152.0.7977.167, files ~412-864 MB [S10]. | Yes in principle: `enable_widevine=True` in CEF's GN args [S5]; component updater delivers the CDM [S5]. CEF's own test page lists Widevine as supported [S5]. | Default CEF builds lack proprietary codecs: maintainer note, "proprietary codecs like MP4 are not supported by default CEF builds" [S5], and CEF's `gn_args.py` sets no `proprietary_codecs`/`ffmpeg_branding` [S5]. Netflix/Spotify-style H.264/AAC content therefore needs a custom CEF build. Not verified: the arm64 CDM actually arrives via component updater (see Unverified). |
| **Qt WebEngine** (Chromium) | Not checked beyond the docs; Qt targets Linux ARM (not Pi-specific). | Supported "if the Widevine CDM plugin has been installed"; not shipped; `--widevine-path` [S6]. Docs say H.264 "requires proprietary audio and video codecs" [S6]. | Same Chromium limits; you provide the CDM yourself. Docs do not mention V4L2 or VA-API [S6]. Also pulls in Qt, which duplicates the gpui toolkit role. |
| **Electron (castLabs fork)** | `linux-arm64` shipped, experimental, from v44.0.0-alpha.9 (2026-08-06); 44.5.1 on 2026-10-03 [S8]. | Yes; fork exists to add Widevine to Electron [S8]. Known limitation: Linux CDM lacks Verified Media Path, so requests report `PLATFORM_UNVERIFIED`; persistent licences are gone on all platforms [S8]. | Heavy runtime (Node + Chromium). Third-party fork. Some services may reject `PLATFORM_UNVERIFIED` clients (not verified). |
| **Firefox** | Pi OS ships `firefox` arm64 (v157.0.1 in trixie) with `libwidevinecdm0` as recommended dep [S4]. | Works on Pi 5 with Google's CDM per a Raspberry Pi engineer [S9]. Mozilla's upstream bug "Widevine support for linux arm64" (1612995) is still NEW/unassigned and cites discussion with Google about "shipping officially" [S9]. | Not an embeddable engine (no stable embedding API); listed for completeness. |
| **WPE WebKit** | Distro packages exist (Raspbian listed); stable release 2.54.1; `WPEPlatform` provides DRM/KMS, Wayland, headless [S7d]. | Only via Thunder/OpenCDM, a build-time option: in `OptionsWPE.cmake`, `ENABLE_ENCRYPTED_MEDIA` defaults to `${ENABLE_EXPERIMENTAL_FEATURES}` and `ENABLE_THUNDER` defaults to `${ENABLE_DEVELOPER_MODE}` [S7a]. Needs Widevine CE-CDM libraries, gated behind licensing [S7b][S7c]. | No primary source found showing Widevine playing on WPE on a Pi. The Igalia write-up (2020) says only Widevine is in the GStreamer JHBuild module set and states no security level [S7b]. Highest integration risk. |

## 4. Hardware video decode on Pi 4 / Pi 5

**What the silicon and kernel provide**

| Codec | Pi 4 (BCM2711) | Pi 5 (BCM2712) |
|---|---|---|
| H.265/HEVC | Listed: "H.265 (4Kp60 decode)" [S11]. Kernel: HEVC stateless V4L2 driver `rpi-hevc-dec` ("Raspberry Pi HEVC / H265 H/W decoder as a stateless V4L2 decoder device"); the same `raspberrypi,hevc-dec` compatible is bound by `bcm2711.dtsi` (Pi 4) and `bcm2712-ds.dtsi` (Pi 5) [S12]. | "4Kp60 HEVC hardware decode" [S11]. Same stateless driver family [S12]. |
| H.264 | Listed: "H.264 (1080p60 decode, 1080p30 encode)" [S11]. Kernel: `bcm2835-codec` V4L2 mem2mem driver talking to VideoCore, exposing H264, MPEG2, MPEG4, MJPEG [S12]. This is a stateful-style device, not stateless. | No hardware decode: "Other CODECs run in software"; Pi docs give CPU load "H264 1080p24 decode ~10-20%" and "1080p60 ~50-60%" [S11]. |
| VP9 | No hardware decoder in the kernel drivers inspected [S12]; Pi docs silent [S11]. | Software ("Other CODECs run in software") [S11]. |
| AV1 | Same as VP9. | Same as VP9. |

I read the Pi docs sentence "Other CODECs run in software" as covering VP9 and AV1 on Pi 5 [S11]. That is an inference from the sentence, not an explicit VP9/AV1 statement. For Pi 4 I rely on the absence of such drivers in `rpi-6.12.y` [S12] plus silence in docs.

**What Chromium can use**

- Upstream Chromium has V4L2 stateless decode delegates for H.264, VP9, HEVC (needs `enable_hevc_parser_and_hw_decoder`) and AV1 (needs `use_av1_hw_decoder`), and a legacy stateful backend [S15]. Raspberry Pi's patch description says that outside ChromeOS "the sole decoder wired up is `V4L2StatefulVideoDecoder`" because the stateless path needs the Request API [S20]. I take that as: stateless decoders (Pi's HEVC block) are not usable from Chromium on Linux today.
- Upstream `kAcceleratedVideoDecodeLinux` is enabled by default only `#if BUILDFLAG(USE_VAAPI)`, and V4L2 is preferred over VA-API only with `kPreferV4L2VideoAcceleration` (default off) [S16]. Raspberry Pi's Chromium handles this by building arm64 with `use_v4l2_codec=true use_vaapi=false use_av1_hw_decoder=true proprietary_codecs=true ffmpeg_branding="Chrome" enable_widevine=true` and patching `kAcceleratedVideoDecodeLinux` to `#if 1` (always enabled) [S20]. Debian's own arm64 build uses `use_vaapi=true` instead [S18]. Official CEF builds set none of the codec flags [S5].
- Pi's patches also show the real-world state of this path: they cap concurrent V4L2 decoders on `bcm2835-codec`, keep polling after `POLLERR`, and allow the GPU sandbox to open the decoder nodes [S20]. The patch text states that on a Pi 4 the HEVC node (`/dev/video19`) would otherwise answer `canPlayType()` "probably" and then fail with `DECODER_ERROR_NOT_SUPPORTED`, which is why stateless nodes are hidden [S20].
- **Pixel format mismatch (separate from the above).** The Pi HEVC driver outputs `V4L2_PIX_FMT_NV12MT_COL128`, `NV12_COL128` and 10-bit variants, a Broadcom column format; it defaults to `NV12MT_COL128` [S12]. GitHub code search for `COL128` in `chromium/chromium` and `GStreamer/gstreamer` returned no hits (code search can be incomplete). Even if the stateless path were wired up, stock Chromium or GStreamer would probably need an extra patch or conversion step.
- The Pi side also needs an HEVC path through the CDM: Google's CDM manifest codec list has no HEVC (`vp8,vp09,avc1,av01`) [S3]. HEVC for DRM content is therefore not available through this CDM. HEVC hardware decode would help only clear (non-DRM) HEVC.

**Net.** For Widevine content on Pi 4: H.264 (what the CDM allows and Pi 4 can decode in hardware) is the only realistic hardware-decode path, via Chromium's stateful V4L2 backend and `bcm2835-codec`. Raspberry Pi ships and maintains exactly that configuration in its Chromium [S20], but I found no source showing DRM-protected H.264 actually going through it (that depends on L3 decrypted frames reaching the hardware decoder, unverified). On Pi 5: expect software H.264/VP9/AV1 at 1080p (Pi docs: H.264 1080p60 ~50-60% CPU) [S11].

## 5. Idle memory / CPU

I found no primary source that measures idle memory or CPU of any of these engines on Pi 4/5. The only architectural fact I can cite is that CEF/Chromium run multiple processes: a browser process, a render process per unique origin by default, and a GPU process [S19]. WPE and Qt multi-process claims were not checked. Treat all numbers as unknown until measured on the target (recipe below).

Suggested measurement on a real Pi 4 and Pi 5 (not done; no Pi hardware available in this session): boot to the m4TV shell with the engine loaded on `about:blank`, wait 60 s, sum PSS of every engine process (`smem -t -P <engine>` or `/proc/*/smaps_rollup`), sample CPU with `pidstat -u -p ALL 5 12`. Repeat with a loaded Widevine-capable page idle (e.g. shaka demo) and while playing 1080p H.264.

## Unverified / not found

- **Widevine L1/L3 definitions from Google.** Google's public Widevine pages do not define security levels (they say documentation depends on licensing) [S14]. The statement "software-secure = L3, TEE = L1" is the widely used mapping but I only have vendor blogs for it, which I excluded as secondary.
- **Component updater delivering a linux-arm64 CDM.** I POSTed update checks for the Widevine component ID to Google's update server for `arm64`, `x64` and `x86`; all three returned `noupdate`, including x64, so my request was not a valid test and proves nothing (no control passed). `https://dl.google.com/widevine-cdm/current.txt` returns 404, so that route is also unusable as a check. Pi's source comment for 32-bit ARM, "No CDM is served for 32-bit ARM; it would download an x64 CRX forever" [S20], shows the component updater can hand out the wrong binary for unsupported arches, so arm64 delivery must be tested on a device, not assumed. Not verified for CEF.
- **That Google "officially announced" Chrome for Arm64 Linux.** Third-party articles say there was no announcement; I did not find a Chrome Releases post. What is verified is the existence of the packages in Google's own repo [S3].
- **Chrome stable version.** Press reports say Chrome 150; the apt index I read says 155.0.8059.84 stable on 2026-10-20 [S3]. Use the index, not the press.
- **Netflix 720p vs 1080p on Linux.** Netflix's page says 1080p for Chrome/Firefox/Edge/Opera on Linux [N1]. Press reports of 720p are not supported by that page.
- **YouTube cap for ordinary (non-DRM) videos and YouTube TV on Pi.** No primary source retrieved; likely codec/decoder-bound rather than DRM-bound but unproven.
- **Whether CEF's current official `linuxarm64` binaries include H.264.** Inferred from `gn_args.py` and a 2021 maintainer comment [S5]; I did not download the 412 MB build to check.
- **Hardware decode of DRM content with L3.** I assumed decrypted L3 output is decoded by the browser's normal decoders, which can be hardware. Not verified in a primary source; the 2020 Igalia post and Chromium code I read do not address it.
- **Idle memory and CPU.** See section 5.
- **WPE WebKit + Widevine on Pi in practice.** No primary evidence either way beyond the build options.
- **Qt WebEngine and Electron on actual Pi 4/5 hardware.** Source statements only.
- **Whether Pi's `libwidevinecdm0` works with CEF.** Its manifest is host-version `10` only and older (2662); not tested.

## Sources

Source versions: S1 and S2 were read on Chromium `main`, and the Linux arm64 library-CDM line and the software-secure-only Linux registration were re-checked at tag 152.0.7977.167. S15-S17 were read on `main` only.

Chromium / Google
- [S1] `third_party/widevine/cdm/widevine.gni`: https://raw.githubusercontent.com/chromium/chromium/main/third_party/widevine/cdm/widevine.gni (`library_widevine_cdm_available` includes `linux` + `arm64`; `enable_widevine` default is true only for Chrome-branded, Chrome-for-Testing, or Android; `bundle_widevine_cdm` needs a branded build). Also `.../widevine/cdm/BUILD.gn`.
- [S2] `chrome/common/media/cdm_registration.cc`: https://raw.githubusercontent.com/chromium/chromium/main/chrome/common/media/cdm_registration.cc (Linux uses `AddSoftwareSecureWidevine`; `Robustness::kSoftwareSecure`; hardware-secure only under `USE_CHROMEOS_PROTECTED_MEDIA`/Android; path layout; bundled-vs-hinted selection).
- [S3] Google Chrome arm64 apt index: https://dl.google.com/linux/chrome/deb/dists/stable/main/binary-arm64/Packages and package https://dl.google.com/linux/direct/google-chrome-stable_current_arm64.deb (contents inspected: `WidevineCdm/manifest.json`, `LICENSE`, `_platform_specific/linux_arm64/libwidevinecdm.so`).
- [S15] `media/gpu/v4l2/BUILD.gn`: https://raw.githubusercontent.com/chromium/chromium/main/media/gpu/v4l2/BUILD.gn
- [S16] `media/base/media_switches.cc`: https://raw.githubusercontent.com/chromium/chromium/main/media/base/media_switches.cc (`kAcceleratedVideoDecodeLinux`, `kPreferV4L2VideoAcceleration`, `kPlatformHEVCDecoderSupport`).
- [S17] `media/media_options.gni`: https://raw.githubusercontent.com/chromium/chromium/main/media/media_options.gni (`enable_platform_hevc`, `platform_has_optional_hevc_decode_support`, `enable_library_cdms`).
- [S14] Widevine developer home (docs gated by licence): https://developers.google.com/widevine

CEF
- [S5] Issue "alloy: Add component updater support for Widevine" https://github.com/chromiumembedded/cef/issues/3149 ; `tools/gn_args.py` (`'enable_widevine': True`) and `patch/patches/linux_chrome_widevine_3149.patch` in https://github.com/chromiumembedded/cef
- [S10] CEF builds index (linuxarm64): https://cef-builds.spotifycdn.com/index.json
- [S19] CEF General Usage, processes: https://github.com/chromiumembedded/cef/blob/master/docs/general_usage.md

Qt
- [S6] Qt WebEngine features (Widevine, DRM): https://doc-snapshots.qt.io/qtwebengine/qtwebengine-features.html

WPE WebKit
- [S7a] `Source/cmake/OptionsWPE.cmake`: https://raw.githubusercontent.com/WebKit/WebKit/main/Source/cmake/OptionsWPE.cmake
- [S7b] Igalia, "Serious Encrypted Media Extensions on GStreamer based WebKit ports" (2020-09-02): https://blogs.igalia.com/xrcalvar/2020/09/
- [S7c] rdkcentral OCDM-Widevine: https://github.com/rdkcentral/OCDM-Widevine (`CMakeLists.txt`, `cmake/FindWideVine.cmake`).
- [S7d] WPE WebKit download page: https://wpewebkit.org/about/get-wpe.html

Raspberry Pi
- [S4] Pi apt archive indexes: https://archive.raspberrypi.com/debian/dists/trixie/main/binary-arm64/Packages.gz and `.../bookworm/...`; package https://archive.raspberrypi.com/debian/pool/main/w/widevine/libwidevinecdm0_4.10.2662.3+1_arm64.deb ; also `rpi-chromium-mods` (carries no Widevine or decode flags).
- [S11] Raspberry Pi processors documentation: https://www.raspberrypi.com/documentation/computers/processors.html
- [S12] Pi kernel `rpi-6.12.y` (files re-checked present on `rpi-6.18.y`, the repo default branch; Pi OS trixie's apt index ships `linux-image-rpi-2712` and `linux-image-rpi-v8` 6.18.50): `drivers/media/platform/raspberrypi/hevc_dec/` (Kconfig, `hevc_d_video.c`) and `drivers/staging/vc04_services/bcm2835-codec/` (Kconfig, `bcm2835-v4l2-codec.c`) in https://github.com/raspberrypi/linux
- [S13] config.txt documentation (`kernel_2712.img`, 16K page size): https://www.raspberrypi.com/documentation/computers/config_txt.html

Others
- [S8] castLabs Electron releases (v44.0.0-alpha.9+wvcus, v44.5.1+wvcus): https://github.com/castlabs/electron-releases/releases
- [S9] Mozilla bug 1612995, "Widevine support for linux arm64": https://bugzilla.mozilla.org/show_bug.cgi?id=1612995
- [S20] Raspberry Pi Chromium source package (debian.tar.xz, 155.0.8059.79-1~deb13u1+rpt1, changelog 2026-10-16): https://archive.raspberrypi.com/debian/pool/main/c/chromium/chromium_155.0.8059.79-1~deb13u1+rpt1.debian.tar.xz (`debian/rules`, `debian/changelog`, `debian/patches/series`, `debian/patches/rpi/0007-Widevide-Add.patch`, `enable-AcceleratedVideoDecodeLinux.patch`, `v4l2-*.patch`, `debian/README.debian`).
- [S18] Debian Chromium `debian/rules`: https://salsa.debian.org/chromium-team/chromium/-/raw/master/debian/rules
- [N1] Netflix Help, supported browsers and resolutions: https://help.netflix.com/en/node/23742
- [N2] Netflix Help, resolution by browser: https://help.netflix.com/en/node/30081
- [SP1] Spotify web player help: https://support.spotify.com/us/article/web-player-help/
- [SP2] Spotify audio quality: https://support.spotify.com/us/article/audio-quality/
- [Y1] YouTube Help, HD/4K movies: https://support.google.com/youtube/answer/3306741
- [Y2] YouTube Help, system requirements: https://support.google.com/youtube/answer/78358

## Reproduction (what I ran)

- `curl -sL https://dl.google.com/linux/chrome/deb/dists/stable/main/binary-arm64/Packages` for the Chrome arm64 index; downloaded the stable `.deb`, parsed the `ar`/`tar.xz` in Python, read `WidevineCdm/manifest.json` and `LICENSE`, and parsed the ELF header/program headers of `libwidevinecdm.so` (machine 0xb7, `PT_LOAD` align 0x10000).
- Downloaded Pi `Packages.gz` for trixie and bookworm, extracted the `libwidevinecdm0` entry and `.deb` the same way.
- `gh api` against `chromiumembedded/cef`, `raspberrypi/linux` (`rpi-6.12.y` and `rpi-6.18.y`), `rdkcentral/OCDM-Widevine`, `castlabs/electron-releases`.
