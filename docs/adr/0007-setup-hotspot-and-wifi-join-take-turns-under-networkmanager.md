# The Setup hotspot and the Wi-Fi join take turns on the one radio, driven by NetworkManager

On the Pi, NetworkManager does everything network-related behind the OS seam: Wi-Fi scan, join, the Setup hotspot and status, and wired DHCP on the dev image. Joining a Wi-Fi and broadcasting the Setup hotspot never run at once: the radio belongs to one of them at a time. `Os::join_wifi` stops the hotspot, and a failed or timed-out join brings it back with the reason, which is what the Shell already does (`hotspot_again`).

## Considered Options

- **Hotspot and station at once on one radio** (AP+STA): the TV could stay on its Wi-Fi while the hotspot runs. Rejected because both then share one channel, it is unproven on the Pi 4 and Pi 5 chips, and nothing in the flows needs it: the first-boot flow loses its phone's connection at the handover anyway (#15), and a failed join already restores the hotspot.
- **wpa_supplicant, hostapd, dnsmasq and systemd-networkd**: the pieces NetworkManager drives for us. Rejected because we would own the glue between four daemons (hotspot DHCP and DNS, the captive-portal catch-all, the join), where Raspberry Pi OS already ships NetworkManager.

## Consequences

- The hotspot that Settings starts on demand disconnects the TV from its Wi-Fi while it is up, so Settings says so before it starts.
- The "hotspot and station on one radio" hardware check is dropped from #34. The contract above is what the Pi implementation and the fake (`FakeOs`) both honour.
- #31 drops systemd-networkd: wired DHCP on the dev flavour is NetworkManager too.
- Whether NetworkManager's hotspot can answer the captive-portal probes (a DNS catch-all and the plain-HTTP redirect) is unchecked, and is part of #34's captive-portal test. Only an iPhone is on hand, so Android is recorded as untested and the "open in your browser" step ships by default.
