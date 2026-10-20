# Serve the Remote from the Pi over pinned HTTPS on the LAN

The Remote is a separate phone web app that the Pi serves, along with the Remote server's API, from one HTTPS origin on the LAN. The certificate is self-signed, and its fingerprint is carried in the Pairing QR code and pinned by the phone. We keep the system LAN-only with no cloud or account, because a phone browser will only call an HTTPS API that it trusts, and a publicly trusted certificate for a LAN-only Pi would need a public domain and an external host.

## Considered Options

- **Remote hosted on an external static host, calling the Pi API cross-origin**: a publicly trusted certificate for the Pi would need a public domain that points at a LAN address. Rejected because it makes the Remote depend on external hosting and DNS, and breaks the LAN-only, no-cloud decision.
- **A private root CA installed on each phone during Pairing**: lets the Pi use a certificate the phone trusts without a public domain. Rejected because it adds a manual install step for every phone.
- **Plain HTTP on the LAN**: simplest. Rejected because the Pairing secret and every command would cross Wi-Fi in the clear, and a PWA needs a secure origin.

## Consequences

- The Pi holds a self-signed certificate, and each phone pins it during Pairing.
- Replacing or renewing the certificate invalidates the pin, so every phone must pair again.
- The Remote needs no public DNS, external host or account.
- The Remote server and the static Remote files share one origin, so the Remote needs no CORS configuration.

## Amendment: the Setup hotspot is a second path for the fingerprint

When the TV has no network it broadcasts a WPA2 Setup hotspot, and the phone does the whole setup there through the Remote: Wi-Fi (including captive-portal logins), Pairing and settings. The hotspot password is shown on the TV as a Wi-Fi QR code and is the trust anchor, so the Remote is served over plain HTTP on the hotspot and the certificate fingerprint reaches the phone over that WPA2 link instead of a Pairing QR. Anyone who knows the hotspot password could pair. We accept that because the password is only shown on the TV screen and the hotspot exists only while the TV has no network or is asked to. On the LAN, Pairing still uses the QR code and the pinned HTTPS above.


## Amendment: a web page cannot pin the certificate, so the QR carries a secret

A web page cannot pin a certificate or read the one it was served with, so the phone does not pin the fingerprint. The viewer taps through the browser's self-signed warning once, and the browser then remembers that exact certificate: a replaced certificate shows the warning again, which is still how a new certificate forces a phone to pair again. To cover the first visit, the Pairing QR code carries a one-time secret in the link fragment (never sent to the server). Phone and TV prove to each other that they know it, with an HMAC over a nonce, before the token is issued, so a man-in-the-middle on the first scan cannot pair. The proof uses `@noble/hashes` (ADR 0004), not WebCrypto. Pairing by typing the PIN stays trust on first use. The fingerprint stays in the link for the viewer to compare, but nothing relies on it.

A page opened past a certificate warning is probably not a secure context, so the Remote cannot count on service workers or "install to home screen". That is unverified and needs a real-phone check. Until then, the "a PWA needs a secure origin" reason above does not hold for the Remote: it is a web page.
