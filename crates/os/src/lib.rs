//! The OS seam (ADR 0003): everything specific to the device (Setup hotspot and Wi-Fi join, audio,
//! power and reboot, CDM fetch location, settings paths) lives behind this crate, with a real
//! implementation for the Pi and a fake for the Dev host. The Shell never calls device APIs itself.
