# m4TV

A custom Raspberry Pi operating system that boots straight into a TV-optimised interface, like a Fire TV, and runs streaming web apps controlled from the TV or from a phone.

## Language

**Shell**:
The TV-optimised interface m4TV boots into. It launches Apps and holds every setting.
_Avoid_: Launcher, UI

**Home**:
The Shell's screen of App tiles, and the first thing shown. The Home key always returns to it from an App.
_Avoid_: Home screen, main menu, launcher

**Tile**:
The focusable entry on Home: one per App, plus one for Settings. Settings is not an App.
_Avoid_: Card, icon, shortcut

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

**Settings**:
Every user-changeable option, each declared once and shared by the Shell and the Remote server. The Shell shows them on its Settings screen (reached from the Settings tile) and is the only writer of the stored values.
_Avoid_: Preferences, options, config

**OS seam**:
The boundary between the Shell and the device: settings paths, power and reboot, version, and later audio, networking and the CDM location. The Pi implements it for real; the Dev host fakes it.
_Avoid_: HAL, platform layer
