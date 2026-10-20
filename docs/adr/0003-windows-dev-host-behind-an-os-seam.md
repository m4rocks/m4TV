# Run the Shell on Windows as a Dev host, with device glue behind an OS seam

The Shell builds and runs as an ordinary Windows process, the Dev host, so it can be debugged without flashing a Pi or running the OS in a VM. Everything specific to the device (Setup hotspot and Wi-Fi join, audio output, power and reboot, CDM fetch location, settings paths) sits behind a seam with a real implementation for the Pi and a fake one for the Dev host. The Dev host runs the real Remote server, pinned HTTPS and Pairing, because that path is the Shell's own code, not device glue.

## Considered Options

- **Test only on the Pi (or a VM of the full image)**: no extra code, but every change needs a flash or a VM boot, and first-boot flows can't be tried quickly. Rejected because the debugging loop would be too slow.
- **Mock the Remote server on the Dev host**: simpler locally. Rejected because it would skip the HTTPS pinning and Pairing path that the Dev host exists to exercise.
- **Build our own CEF on Windows too**: closer Engine fidelity. Rejected because it adds a second custom CEF build to maintain; DRM and H.264 playback are verified on the Pi.

## Consequences

- Shell code may not call device APIs directly; it goes through the seam, and CI compiles the Windows target so Linux-only code can't creep in.
- The Dev host uses stock CEF and a keyboard stand-in for the TV remote; it proves behaviour and look, never performance. The Pi 4 bars and the frame-path spike are measured only on hardware.
- The fake network can simulate "no network", so first-boot and Setup hotspot flows are testable without a Pi.
