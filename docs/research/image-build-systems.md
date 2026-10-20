# Image build systems for a minimal Pi 4 / Pi 5 Wayland kiosk image

Issue: m4rocks/m4TV#5 (parent #2, blocks #11). Researched 2026-10-20.

Question: compare Buildroot, Yocto and pi-gen for a minimal Pi 4 / Pi 5 image that boots into a Wayland kiosk compositor running a Rust app. Cover Rust, Mesa/Vulkan, packaging of a non-Rust Engine (Chromium/CEF or WPE), image size, build time, A/B OTA, and one-person maintenance burden.

Method: primary sources only (project source trees, official docs, first-party indexes). Each claim carries its source. Anything that I could not trace to a primary source is in "Not verified" at the end and is not used in the summary. Statements marked **(inference)** are my reading of the cited facts, not something the source says.

A fourth option turned up in the sources and is included because Raspberry Pi's own docs point at it for A/B: **rpi-image-gen** (the Pi-side successor-in-spirit to pi-gen, not described as a replacement by its README).

## Summary

| | Buildroot | Yocto (+ meta-raspberrypi) | pi-gen | rpi-image-gen |
|---|---|---|---|---|
| Base | Source-built, no package manager | Source-built, package feeds optional | Debian/Raspbian debootstrap | Debian prebuilt packages (mmdebstrap) |
| Pi 4 / Pi 5 | in-tree defconfigs for both | `raspberrypi4-64`, `raspberrypi5` machines | official Pi OS builder (its README names Pi 4 but not Pi 5) | device classes (docs name pi4 and cm5 as examples) |
| Rust (target app) | `cargo-package` infra, rustc 1.98.1 | `cargo.bbclass`, rustc 1.99.0 | Debian rustc 1.85.1 (Trixie) | same as pi-gen (Debian) |
| Mesa v3d / v3dv (Vulkan) | `BR2_PACKAGE_MESA3D_VULKAN_DRIVER_BROADCOM` | `PACKAGECONFIG` `broadcom` + `v3d` | Debian `mesa-vulkan-drivers` ships `libvulkan_broadcom.so` | same as pi-gen |
| Compositor | cage, sway, weston in-tree | cage, sway, labwc via meta-wayland (not oe-core) | cage, labwc, sway, weston in Debian | cage example exists |
| Chromium | **not in tree** | meta-browser `chromium-ozone-wayland` | Debian `chromium` package | `chromium` + `cage` kiosk example |
| WPE / Cog | `wpewebkit` + `cog` in tree | meta-webkit / meta-wpe | Debian `cog` + `libwpewebkit-2.0-1` | via Debian |
| CEF | no package | no recipe found in layer index | no package | no package |
| A/B OTA | RAUC, SWUpdate, Mender packages; Pi bootloader glue is DIY | meta-rauc + meta-rauc-community Pi layer; meta-swupdate; Mender | nothing built in; mender-convert can retrofit | A/B slot layout + Pi Connect OTA (experimental) |

## 1. Buildroot

**Pi 4 / Pi 5 support.** The tree has `raspberrypi4_64_defconfig` and `raspberrypi5_defconfig`, plus CM4 IO and CM5 IO variants ([config list](https://gitlab.com/buildroot.org/buildroot/-/tree/master/configs), [board readme](https://gitlab.com/buildroot.org/buildroot/-/raw/master/board/raspberrypi/readme.txt)). Both use a Bootlin external glibc toolchain and a Raspberry Pi kernel pinned to a specific commit (bcm2711 / bcm2712 defconfig) ([pi4_64](https://gitlab.com/buildroot.org/buildroot/-/raw/master/configs/raspberrypi4_64_defconfig), [pi5](https://gitlab.com/buildroot.org/buildroot/-/raw/master/configs/raspberrypi5_defconfig)). The kernel pin means kernel bumps are manual **(inference)**.

**Rust.** Buildroot has a Cargo package infrastructure; dependencies are vendored into the download tarball so the build is offline ([adding-packages-cargo.adoc](https://gitlab.com/buildroot.org/buildroot/-/raw/master/docs/manual/adding-packages-cargo.adoc)). Two host compilers exist: `rust` (builds rustc from source) and `rust-bin` (prebuilt), both at 1.98.1 on master ([rust.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/rust/rust.mk), [rust-bin.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/rust-bin/rust-bin.mk)). aarch64 + glibc is treated as a Tier 1 target ([rustc/Config.in.host](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/rustc/Config.in.host)).

**Mesa / Vulkan.** `mesa3d` exposes `BR2_PACKAGE_MESA3D_GALLIUM_DRIVER_V3D` ("Broadcom VC6/7 (rpi4/5) GPUs") and `BR2_PACKAGE_MESA3D_VULKAN_DRIVER_BROADCOM` on arm/aarch64 ([mesa3d/Config.in](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/mesa3d/Config.in)). Upstream Mesa documents V3D (GLES 3.1) and V3DV (Vulkan) as used on Pi 4 (V3D 4.2) and Pi 5 (V3D 7.1) ([Mesa V3D docs](https://docs.mesa3d.org/drivers/v3d.html)).

**Compositor.** `cage` ("Kiosk compositor for Wayland, which displays a single maximized application at a time"), `sway`, `weston` and `wlroots` are in the package tree; `cage` needs EGL, GLES, GBM and udev ([cage/Config.in](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/cage/Config.in), [package list via GitLab API](https://gitlab.com/api/v4/projects/buildroot.org%2Fbuildroot/repository/tree?path=package)). `labwc` is not in the tree.

**Non-Rust Engine.**
- WPE: `wpewebkit` 2.50.5 and `cog` 0.18.5 are packages. `wpewebkit` requires GLES + EGL, gcc >= 11 and pulls in ICU, libsoup3, etc.; `cog` has Wayland (fdo) and DRM backends ([wpewebkit/Config.in](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/wpewebkit/Config.in), [wpewebkit.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/wpewebkit/wpewebkit.mk), [cog/Config.in](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/cog/Config.in), [cog.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/cog/cog.mk)).
- Chromium / CEF / Firefox / qt6webengine: no `package/chromium`, `package/libcef`, `package/qt6webengine` or `package/firefox` directory exists on master (404 on each `Config.in`; the full package-directory listing contains none of them). A Chromium or CEF engine would be an out-of-tree package.
- Prebuilt CEF does exist for Linux ARM64: the CEF builds index lists a `linuxarm64` stable build 152.0.7977.167 whose "minimal" distribution archive is about 413 MB and "standard" about 864 MB ([cef-builds index.json](https://cef-builds.spotifycdn.com/index.json), size fields, read 2026-10-20). Sizes are archive sizes, not installed size.

**Image size.** Both Pi defconfigs size the ext4 rootfs at 120 MB (a cap, not a measurement) ([pi5 defconfig](https://gitlab.com/buildroot.org/buildroot/-/raw/master/configs/raspberrypi5_defconfig)). No primary source gives a measured size for a Wayland + Mesa + Rust (or + WPE) image. See "Not verified".

**Build time.** The manual states that toolchain build time "is often very significant in the overall build time" and that rebuilding the toolchain is needed on `make clean` (manual sections 6.1.1/6.1.2, [manual](https://buildroot.org/downloads/manual/manual.html)). Top-level parallel build is off by default; per-package parallelism defaults to CPUs + 1; top-level parallel is "a very experimental feature" (manual sec. 4, 8.13). The Pi defconfigs use an external Bootlin toolchain, which avoids the toolchain build. No primary benchmark exists.

**Rebuild semantics (maintenance-relevant).** "Buildroot does not attempt to detect what parts of the system should be rebuilt" and removing a package "is unsupported by Buildroot without rebuilding from scratch" (manual sec. 8.2, 8.3). Overlay/post-build script changes need only `make` (manual sec. 8.2).

**OTA / A/B.**
- Packages: `rauc` 1.15.2, `swupdate` 2026.05.1, `mender` 3.5.3 ([rauc.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/rauc/rauc.mk), [swupdate.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/swupdate/swupdate.mk), [mender.mk](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/mender/mender.mk)).
- Mender's Buildroot readme: A/B "requires additional integration in the bootloader... board specific. Currently supported bootloaders are GRUB and U-boot", with the only in-tree reference being x86_64 EFI ([mender readme](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/mender/readme.txt)).
- RAUC supports barebox, grub, uboot, efi, custom and noop bootloader backends; `custom` takes your own handler script ([RAUC reference](https://rauc.readthedocs.io/en/latest/reference.html), [integration](https://rauc.readthedocs.io/en/latest/integration.html)). SWUpdate supports U-Boot, GRUB, EFI Boot Guard and a RAM fake ([SWUpdate bootloader interface](https://github.com/sbabic/swupdate/blob/master/doc/source/bootloader_interface.rst)). Neither document mentions Raspberry Pi tryboot.
- Buildroot ships a U-Boot Pi defconfig only for CM4 IO (`raspberrypicm4io_64_uboot_defconfig`); there is none for Pi 5 ([config list](https://gitlab.com/buildroot.org/buildroot/-/tree/master/configs)).
- The Pi firmware has a native A/B mechanism: `autoboot.txt` with `boot_partition`, `tryboot_a_b` and a `[tryboot]` filter, triggered by `reboot "0 tryboot"` ([Pi autoboot.adoc](https://github.com/raspberrypi/documentation/blob/develop/documentation/asciidoc/computers/config_txt/autoboot.adoc)). **(inference)** RAUC's `custom` backend plus tryboot would avoid U-Boot entirely, but no source shows anyone doing this on Buildroot; it would be my own glue.
- Buildroot's FAQ states the philosophy: whole-rootfs upgrade, not binary packages ([faq-troubleshooting.adoc](https://gitlab.com/buildroot.org/buildroot/-/raw/master/docs/manual/faq-troubleshooting.adoc)). This fits A/B.

**Maintenance.** Quarterly releases (Feb/May/Aug/Nov); since 2025.02 every odd-year Feb release is LTS, supported three years; non-LTS is supported only until the next release ([release-engineering.adoc](https://gitlab.com/buildroot.org/buildroot/-/raw/master/docs/manual/release-engineering.adoc)). Current LTS is 2025.02.x, EOL when 2028.02 is released (same source). Pi support rides on mainline defconfigs maintained by the Buildroot community, no separate layer to track **(inference)**.

## 2. Yocto (openembedded-core + meta-raspberrypi)

**Pi 4 / Pi 5 support.** meta-raspberrypi defines `raspberrypi4-64` and `raspberrypi5` machines (Pi 5 uses `kernel_2712.img`, `vc4-kms-v3d`) ([machine dir](https://git.yoctoproject.org/meta-raspberrypi/tree/conf/machine), [raspberrypi5.conf](https://git.yoctoproject.org/meta-raspberrypi/plain/conf/machine/raspberrypi5.conf)). It is a "Yocto Project Compatible" layer, sponsored by balena, with branches including `scarthgap` and `wrynose`; master declares `LAYERSERIES_COMPAT = "blacksail"` ([README](https://git.yoctoproject.org/meta-raspberrypi/plain/README.md), [layer.conf](https://git.yoctoproject.org/meta-raspberrypi/plain/conf/layer.conf), [branches](https://github.com/agherzan/meta-raspberrypi/branches)).

**Rust.** Rust is in openembedded-core: `rust_1.99.0.bb`, `cargo_1.99.0.bb`, `cargo.bbclass`, and `cargo-update-recipe-crates.bbclass` ("update the list of crates in SRC_URI by reading Cargo.lock"; `bitbake -c update_crates`) ([oe-core rust dir](https://git.openembedded.org/openembedded-core/tree/meta/recipes-devtools/rust), [cargo.bbclass](https://git.openembedded.org/openembedded-core/plain/meta/classes-recipe/cargo.bbclass), [cargo-update-recipe-crates.bbclass](https://git.openembedded.org/openembedded-core/plain/meta/classes-recipe/cargo-update-recipe-crates.bbclass), [README-rust.md](https://git.openembedded.org/openembedded-core/plain/meta/recipes-devtools/rust/README-rust.md)).

**Mesa / Vulkan.** oe-core `mesa.inc` has `PACKAGECONFIG` options `v3d`, `vc4`, `broadcom` (Vulkan driver), `vulkan`, `gbm`, `gles`, `egl`, `wayland` ([mesa.inc](https://git.openembedded.org/openembedded-core/plain/meta/recipes-graphics/mesa/mesa.inc)).

**Compositor.** Per the OE layer index, `cage`, `sway`, `labwc` and `wlroots` live in the `meta-wayland` layer (not oe-core / meta-openembedded) ([cage](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=cage), [sway](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=sway), [labwc](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=labwc)). Weston is in oe-core **(not checked here)**.

**Non-Rust Engine.**
- Chromium: `meta-browser` provides `chromium-ozone-wayland` (layer index) and says it "strives to always follow the latest stable Linux release". It needs clang (GCC unsupported) via `meta-clang`, runs target binaries under QEMU for V8, and recommends "a x86-64 machine with at least 16GB RAM". Only master and current LTS branches are supported ([meta-chromium README](https://github.com/OSSystems/meta-browser/blob/master/meta-chromium/README.md), [layer index](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=chromium-ozone-wayland)).
- WPE: `wpewebkit` and `cog` exist in `meta-webkit` (Igalia) and `meta-wpe` ([meta-webkit README](https://github.com/Igalia/meta-webkit/blob/main/README.md), [layer index: wpewebkit](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=wpewebkit), [cog](https://layers.openembedded.org/layerindex/branch/master/recipes/?q=cog)).
- CEF: a layer-index search for `cef`/`libcef` found no CEF recipe (only unrelated `cefglue`). Prebuilt linuxarm64 CEF is available as above.

**Image size / build time.** Primary numbers are only host requirements: building `core-image-sato` for `qemux86-64` needs "at least 140 Gbytes of free disk space" and the docs say it builds with "as little as 32 Gbytes of RAM on an old system with 4 CPU cores"; the quick-start says "At least 32 Gbytes of RAM" ([system requirements](https://docs.yoctoproject.org/ref-manual/system-requirements.html), [quick build](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html)). The quick-start gives no build duration. A shared-state (sstate) cache mirror can "significantly speed up your build" ([quick build](https://docs.yoctoproject.org/brief-yoctoprojectqs/index.html)). Chromium alone wants >= 16 GB RAM (above). No primary source gives a minimal Pi image size.

**OTA / A/B.**
- `meta-rauc` provides `rauc`; `meta-rauc-community` has a `meta-rauc-raspberrypi` layer with a dual-slot `wks`, using U-Boot (`RPI_USE_U_BOOT = "1"`). Its README says it is "just an example layer... for demo purpose only", compat series `wrynose blacksail`, and that Pi 5 additionally needs `meta-lts-mixins` branch `scarthgap/u-boot` ([README](https://github.com/rauc/meta-rauc-community/blob/master/meta-rauc-raspberrypi/README.rst), [layer.conf](https://github.com/rauc/meta-rauc-community/blob/master/meta-rauc-raspberrypi/conf/layer.conf)). The Pi 5 note may be stale; I did not verify it against current meta-lts-mixins.
- `meta-swupdate` and Mender layers also exist in the index (RAUC confirmed; the others not individually read).

**Maintenance.** Per the Yocto releases wiki (fetched via a summarising tool, so re-check before relying): LTS releases are supported 4 years; Scarthgap 5.0 to Apr 2028, Wrynose 6.0 to Apr 2030; non-LTS about 6-7 months ([Releases](https://wiki.yoctoproject.org/wiki/Releases)). Supported build hosts are a fixed distro list; "validation is not performed against build hosts using WSL 2" ([system requirements](https://docs.yoctoproject.org/ref-manual/system-requirements.html)). Layer stack here is oe-core + bitbake + meta-yocto-or-distro + meta-raspberrypi + meta-wayland + meta-browser/meta-webkit + meta-clang + meta-rauc, each with its own branch policy **(inference from the sources above)**.

## 3. pi-gen

**What it is.** "Tool used to create the official Raspberry Pi OS images" ([repo](https://github.com/RPi-Distro/pi-gen); last push 2026-10-06 via GitHub API). It debootstraps Debian/Raspbian; stages: 0 debootstrap (non-bootable), 1 minimal bootable, 2 Lite, 3 desktop base, 4 "Normal" ("meant to fit on a 4GB card"), 5 Full. Default `RELEASE` is `trixie`; 64-bit images are built from the `arm64` branch; stages can be skipped/replaced via `SKIP` files ([README](https://github.com/RPi-Distro/pi-gen/blob/master/README.md)).

**Rust.** Rust is not a pi-gen concern; the app can be built outside the image (cross or native) and copied in **(inference)**. For reference, Debian Trixie ships `rustc`/`cargo` 1.85.1 ([rustc](https://packages.debian.org/trixie/rustc)), well behind Buildroot (1.98.1) and Yocto (1.99.0).

**Mesa / Vulkan.** Debian Trixie `mesa-vulkan-drivers` 25.0.7 contains `libvulkan_broadcom.so` and `broadcom_icd.json` for arm64; package download is 10.9 MB ([filelist](https://packages.debian.org/trixie/arm64/mesa-vulkan-drivers/filelist), [download](https://packages.debian.org/trixie/arm64/mesa-vulkan-drivers/download)).

**Compositor and Engine.** All from apt in Trixie: `cage` 0.2.0, `labwc` 0.8.3, `sway` 1.10.1, `chromium` 155.0.8059.79, `cog` 0.18.4, `libwpewebkit-2.0-1` 2.48.3 ([cage](https://packages.debian.org/trixie/cage), [labwc](https://packages.debian.org/trixie/labwc), [sway](https://packages.debian.org/trixie/sway), [chromium](https://packages.debian.org/trixie/chromium), [cog](https://packages.debian.org/trixie/cog), [wpewebkit](https://packages.debian.org/trixie/libwpewebkit-2.0-1)). This is by far the cheapest way to get Chromium or WPE: no compile.

**Image size.** Reference data point (not a minimal kiosk): the official Raspberry Pi OS Lite arm64 Trixie image (2026-10-06) is 550,466,056 bytes compressed and 3,078,619,136 bytes extracted ([Pi imager OS list](https://downloads.raspberrypi.com/os_list_imagingutility_v4.json)). The extracted figure is the disk-image size, which includes free space **(inference)**. The README warns `WORK_DIR` holds "a complete copy of the target system for each build stage... tens of gigabytes".

**Build time.** README gives none. It says native builds are not supported (outside a native environment it uses qemu), and Docker builds are supported ([README](https://github.com/RPi-Distro/pi-gen/blob/master/README.md)).

**OTA / A/B.** pi-gen has no A/B feature. Mender's `mender-convert` can retrofit A/B to a Pi OS image; its configs include `raspberrypi4_trixie_64bit_config` and `raspberrypi5_trixie_64bit_config` ([mender-convert configs](https://github.com/mendersoftware/mender-convert/tree/master/configs/raspberrypi/uboot/debian)). Debian also ships `rauc` 1.13 and `swupdate` 2024.12.1 ([rauc](https://packages.debian.org/trixie/rauc), [swupdate](https://packages.debian.org/trixie/swupdate)).

**Maintenance.** The README makes no support or release-cadence commitments. Pi OS is a Debian derivative; the image tracks Debian Trixie security updates via apt. Image rebuilds are cheap, but whole-OS drift (apt-managed) means the "tested image" guarantee Buildroot argues for is weaker unless you freeze/snapshot packages **(inference)**.

## 4. rpi-image-gen (Raspberry Pi's newer tool)

- README: "a tool for creating custom software images for Raspberry Pi devices"; benefits listed include "Fast builds - Uses pre-built packages instead of building from source"; uses `bdebstrap`, `mmdebstrap`, `genimage`; runs unprivileged; "under active development"; supported hosts are Debian Bookworm/Trixie arm64, containers/QEMU "not formally supported". It points to pi-gen for the Pi OS distribution itself ([README](https://github.com/raspberrypi/rpi-image-gen/blob/master/README.adoc); repo pushed 2026-10-20 per GitHub API).
- Kiosk example: `examples/webkiosk` installs `cage`, `chromium`, `seatd` and a systemd unit running `chromium --kiosk` on Wayland, "also uses the AB image layout" ([webkiosk README](https://github.com/raspberrypi/rpi-image-gen/blob/master/examples/webkiosk/README.md), [mykiosk.yaml](https://github.com/raspberrypi/rpi-image-gen/blob/master/examples/webkiosk/layer/mykiosk.yaml)).
- A/B layout: configs `trixie-minbase-ab.yaml` / `bookworm-minbase-ab.yaml`; `image/gpt/ab_userdata` gives read-only erofs/ext4 root in slots A/B, shared persistent partition, per-slot `/var` ([config dir](https://github.com/raspberrypi/rpi-image-gen/tree/master/config), [ab_userdata image.adoc](https://github.com/raspberrypi/rpi-image-gen/blob/master/image/gpt/ab_userdata/image.adoc)).
- OTA transport: `examples/ota` builds a `.update.tar.zst` bundle deployed through "Raspberry Pi Connect's experimental remote update capability" ([ota README](https://github.com/raspberrypi/rpi-image-gen/blob/master/examples/ota/README.md)). That makes the update channel a Raspberry Pi service dependency unless you write your own updater against the slot layout **(inference)**.
- The Pi docs explicitly point to rpi-image-gen for "an automated solution" to A/B and warn "Building a resilient, production-ready A/B update system requires dedicated partition layouts" ([autoboot.adoc](https://github.com/raspberrypi/documentation/blob/develop/documentation/asciidoc/computers/config_txt/autoboot.adoc)).

## 5. Cross-cutting facts

- **Pi native A/B (tryboot).** `autoboot.txt` is "a native bootloader and firmware capability", independent of third-party multi-boot managers; example layout is bootloader partition + two boot FATs + two roots + data ([autoboot.adoc](https://github.com/raspberrypi/documentation/blob/develop/documentation/asciidoc/computers/config_txt/autoboot.adoc)). The doc excerpt I read does not state which Pi models support tryboot. Note this is distinct from the Pi 5 *EEPROM* A/B, which is "compatible only with the Raspberry Pi 5, Raspberry Pi Compute Module 5, and Raspberry Pi 5 keyboard computer" ([Pi hardware docs](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)).
- **WPE on the Pi GPU.** Buildroot's `wpewebkit` requires GLES + EGL and Cog supports both a Wayland (fdo) and a DRM backend; the DRM backend needs EGL + GBM ([Config.in files above](https://gitlab.com/buildroot.org/buildroot/-/raw/master/package/cog/Config.in)). Mesa documents a conformant GLES 3.1 driver on Pi 4/5 ([Mesa V3D](https://docs.mesa3d.org/drivers/v3d.html)). Whether WPE uses Vulkan: no source found.
- **Vulkan for the Rust app.** On every option the Pi Vulkan driver is Mesa v3dv, which is a Mesa build option or package (rows in the Summary). Buildroot also has `vulkan-loader`, `vulkan-headers`, `vulkan-tools` packages ([package list](https://gitlab.com/api/v4/projects/buildroot.org%2Fbuildroot/repository/tree?path=package)). Which Vulkan API version v3dv reports on Pi 4 vs Pi 5 was not read from a primary source.

## 6. One-person maintenance read (inference from the above)

- **pi-gen / rpi-image-gen:** lowest effort to a working Chromium or WPE kiosk (apt packages, cage example, A/B layout shipped). Costs: Debian Rust is old (build your app elsewhere), OTA path is Pi Connect or DIY, packages float with Debian unless pinned, and rpi-image-gen is "under active development".
- **Buildroot:** one tree, one `make`, small and reproducible, up-to-date Rust and Mesa, WPE+Cog in tree, 3-year LTS. Costs: no Chromium/CEF package (you write and maintain a Chromium or CEF package, or drop in prebuilt CEF), bootloader/A-B glue for Pi is DIY (no Pi 5 U-Boot defconfig; Mender only GRUB/U-Boot), and "no automatic rebuild detection".
- **Yocto:** most complete off-the-shelf answer (meta-browser Chromium, meta-webkit WPE, meta-rauc Pi demo layer, 4-year LTS) but the heaviest: 140 GB disk / 32 GB RAM class host, a stack of ~6-8 layers to keep branch-aligned, Chromium needs clang + 16 GB RAM, and the Pi RAUC layer calls itself a demo.
- **Engine choice drives it:** WPE favors Buildroot or Yocto; Chromium favors Debian-based tooling or Yocto; CEF has no packaged recipe in any of the three, so it is a prebuilt-blob exercise everywhere.

## Not verified (no primary source found, or not read)

1. Build times for any of the three systems on a defined machine. No primary source gives one; Yocto and Buildroot docs only give qualitative or host-requirement statements.
2. Measured image size of a minimal Wayland + Mesa + Rust (or + WPE / + Chromium) image in Buildroot or Yocto. Only the 120 MB Buildroot rootfs cap and the 3.08 GB Pi OS Lite figure were found.
3. Which Pi models support firmware `tryboot` (Pi 4 vs Pi 5): the part of the Pi docs that would say so was not read.
4. Whether RAUC / SWUpdate work with Pi `tryboot` without U-Boot: no source; only RAUC's generic `custom` backend is documented.
5. Current accuracy of the meta-rauc-raspberrypi Pi 5 dependency on `meta-lts-mixins scarthgap/u-boot`; Mender's official (docs.mender.io) support statement for Pi 5. The mender-convert Pi 5 config exists in-repo, but "tested" status lives on Mender Hub, which I did not read.
6. WPE performance and current Pi 4 status on WPE (only old 2019-2021 mailing-list threads surfaced, not used), and whether WPE uses Vulkan.
7. CEF on Wayland/Ozone on current CEF: only 2017-2019 Collabora posts and an old CEF branch readme surfaced (not primary or not current). CEF Pi 4/5 suitability is untested here.
8. Yocto LTS dates and the "blacksail / wrynose / Garburn" series come from a summarised fetch of the Yocto wiki, which I could not cross-check against a second primary page.
9. Weston in oe-core (assumed, not opened); `meta-swupdate` and Mender Yocto layer contents (listed in the layer index only).
10. Whether Buildroot's default host Rust is `rust-bin` or `rust` (both exist; default not read).
