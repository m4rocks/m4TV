# m4TV

A custom Raspberry Pi operating system that boots straight into a TV-optimised interface, like a Fire TV, and runs streaming web apps controlled from the TV or from a phone.

## Language

**Shell**:
The TV-optimised interface m4TV boots into. It launches Apps and holds every setting.
_Avoid_: Launcher, home screen, UI

**App**:
A web service (such as YouTube, Spotify or Netflix) that m4TV presents as a TV application.
_Avoid_: Channel, site, service

**Engine**:
The web browser technology that runs an App's pages and plays its protected video.
_Avoid_: Browser, renderer, webview

**Overlay**:
Shell interface drawn on top of an App while the App keeps playing, such as a volume indicator or a pairing PIN.
_Avoid_: Popup, HUD, toast

**Remote**:
The phone web app that controls the TV: input, text entry, launching Apps and changing every setting. It is a separate app from the Remote server.
_Avoid_: Controller, companion app

**Remote server**:
The device-side service that the Remote talks to to control the Shell and change its settings.
_Avoid_: Backend, Remote API

**Pairing**:
Linking a phone to the TV by scanning a QR code or entering a PIN, so the Remote is allowed to control it.
_Avoid_: Login, connecting

**Dev host**:
The Shell running as an ordinary window on a developer's Windows machine instead of on the device, so it can be tried and debugged without the OS. It shows how the Shell looks and behaves, not how well the device performs.
_Avoid_: Emulator, simulator, dev mode, desktop run

**Setup hotspot**:
The Wi-Fi network the TV broadcasts when it has no network, or when asked to, so a phone can set the TV up before it has a network: choosing the Wi-Fi to join, Pairing, and changing settings through the Remote.
_Avoid_: Access point, AP mode, setup mode
