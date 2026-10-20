# What does each streaming app need to launch in TV mode?

Research for GitHub issue m4rocks/m4TV#6. Retrieved 2026-10-20. Primary sources only (the service's own docs, terms and help pages, plus direct HTTP probes of the live service). Each claim carries its source. Anything I could not confirm from a primary source is in "Unverified / gaps" at the end.

## Summary table

| | YouTube | Spotify | Netflix |
|---|---|---|---|
| TV / 10-foot mode exists as a web page | Yes: `https://www.youtube.com/tv` | No documented one. TV use is via native apps and Spotify Connect | No documented one. TV use is via native apps; web is the normal desktop site |
| Needs a special user agent | Yes: a TV-class UA (certified devices send `Cobalt/...`) | n/a | n/a |
| DRM | Widevine (`com.widevine.alpha`) via EME; L1 required of certified devices | Web player and Web Playback SDK need EME (Widevine) | Not published for web. Browser table only gives resolution caps |
| Codecs | VP9, H.264/AVC, AAC, Opus (AV1 optional) over MSE | Not published | Not published |
| Sign-in with phone | Yes: QR code or `yt.be/activate` + code | Yes: Spotify Connect from phone, or PIN at `spotify.com/pair` | Yes: QR code or `netflix.com/tv2` code |
| Terms on custom clients | Forbid circumventing/modifying the Service; forbid use outside the Service as provided | Forbid circumventing technology, framing, modifying, scraping | Forbid circumventing content protections, inserting code, scraping; software only for "Netflix supported devices" |

## YouTube (YouTube on TV, "Leanback")

### URL and user agent

- Certified devices must load the app from `https://www.youtube.com/tv`, with query parameters and fragments appended as required (requirement 8.1). Example launch URLs include `https://www.youtube.com/tv?launch=remote` and `https://www.youtube.com/tv?v=9bZkp7q19f0` for a specific video ID (requirement 9.4). Source: [YouTube Software Requirements for CE Devices 2020, version 2019/11/06](https://developers.google.com/youtube/devices/living-room/files/pdf-guides/Revised_YouTube_Software_Requirements_for_CE_Devices_2020.pdf) (the URL now 404s on developers.google.com; I read the Wayback Machine copy of it).
- The same spec says the other Living Room apps, `https://kids.youtube.com` and `https://tv.youtube.com` (the paid YouTube TV live-TV service), are subject to the same technical requirements unless called out. Same source, "Purpose of this Document".
- The spec requires the browser engine to be Cobalt (requirement 0.1) and the User-Agent `Browser_Name` field to be `Cobalt` (requirement 9.2.1). Its example UA is `Mozilla5.0 (Linux) Cobalt/11.119147-gold (unlike Gecko) Starboard6,Google, _TV_MK120P_2020/V3.02 (Google, 43D2700K-US, Wireless)` (requirement 9.2.3; the example is printed with a missing `/` after `Mozilla`). Same source.
- Cobalt is Google's "high-performance, small-footprint platform that implements a subset of HTML5/CSS/JS" meant to "Support YouTube HTML5 applications on Living Room devices" and run "the YouTube TV app". Source: [developers.google.com/youtube/cobalt](https://developers.google.com/youtube/cobalt) (cobalt.dev redirects here). Source code: [github.com/youtube/cobalt](https://github.com/youtube/cobalt).
- Certified devices also sign a `certification_scope` plus a secret key (HMAC-SHA256) that is "sent as a URL parameter to verify that the device has been certified and authorized to use the YouTube service" (requirement 9.1, implementation note). Same CE spec. This means a spoofed UA cannot reproduce full device authentication; whether youtube.com/tv enforces it today is not documented.
- Direct probe of the live service on 2026-10-20 (`curl` GET of `https://www.youtube.com/tv`, my own observation, not a documented guarantee):
  - With a Windows Chrome desktop UA, the server returns a 5.7 KB interstitial page. Its text reads "You are being directed to youtube.com" and "For a better experience, check your device's app store for the official YouTube app."
  - With three TV-style UAs it returned the full ~225 KB TV app (title "YouTube pe TV"): a Cobalt-format UA, a Tizen `SMART-TV` UA, and a desktop Chrome UA with a bare `SmartTV` token appended. I did not execute the app, so I cannot say whether playback or sign-in then works.

### DRM and codecs

All from the CE spec above, section 2.4.
- Must implement Encrypted Media Extensions (W3C CR 18 Sep 2017) for the `com.widevine.alpha` key system, OEMCrypto 15 or greater, with at least 16 `MediaKeySession` objects and 16 keys per session (2.4.1).
- Devices implementing Widevine "MUST implement Widevine Level 1 content protection with secure hardware decode" (2.4.1). The robustness table lists Widevine L1 for the main video formats at all resolutions and Widevine L3 for the two rows whose format names are lost in the PDF extraction (probably the secondary-stream formats).
- YouTube uses WebM subsample encryption for all protected VP9 Profile 0 and Profile 2 streams (2.4.1).
- Container/codec list in the key-system table: WebM/VP9 Profile 0, WebM/VP9 Profile 2 (10-bit), MP4/H264 (AVC), MP4/AAC, WebM/Opus (2.4.1). Above 1920x1080 only VP9 is supported.
- Must implement Media Source Extensions (W3C CR 17 Nov 2016) with source buffers of at least 30 MB video + 5 MB audio for 1080p devices, 50 MB for 4K, 80 MB for 4K HDR (2.4.2). `isTypeSupported` must accept `width`, `height`, `framerate`, `bitrate`, `cryptoblockformat`, and `eotf` for HDR (2.4.2).
- AV1 is optional: "The isTypeSupported method MAY support the AV1 codec string `av01.01.61.<bitdepth>`" (13.2, HDR section).
- A second, concurrent H.264 Main Profile 432x240 video decoder is required, with no DRM requirement on that stream (2.1.1).
- A newer revision of the spec exists but is gated: the Living Room certification pages say the partner site needs a `corp-partner.google.com` account ([access page](https://developers.google.com/youtube/devices/living-room/access/accessing-lr-partnership-resources); [Living Room home](https://developers.google.com/youtube/devices/living-room)). The 2020 text above is the latest I could read, so treat codec/Widevine levels as possibly stale (e.g. AV1 may now be required).
- For m4TV (not a certified device), what matters in practice is that the embedded browser provides MSE, EME with Widevine, and VP9/H.264/AAC/Opus decode. The L1 hardware requirement binds certified devices; this spec does not say what YouTube serves a non-certified client.

### Sign-in with remote or phone

- YouTube TV app sign-in: open the app, choose phone sign-in, then "Scan the QR code, or go to yt.be/activate on your phone, tablet, or computer, then enter the code on the screen." The TV and phone should be on the same Wi-Fi if phone sign-in fails. Source: [YouTube Help, "Sign in to YouTube on your TV"](https://support.google.com/youtube/answer/3015415) (the page I fetched; its title was not captured).
- Cast/DIAL: devices must also implement DIAL 2.2 (CE spec, section 7), which is how the phone app's "cast to TV" works. The spec says no `launch=` parameter should be sent when the app is launched via DIAL (9.4 note).

### Terms for custom or embedded browsers

Source for all: [YouTube Terms of Service, "Permissions and Restrictions"](https://www.youtube.com/static?template=terms&gl=US&hl=en).
- You may not "access, reproduce, download, distribute, transmit, broadcast, display, sell, license, alter, modify... or otherwise use any part of the Service or any Content except: (a) as expressly authorized by the Service; ... or (b) with prior written permission from YouTube" (item 1).
- You may not "circumvent, disable, fraudulently engage with, or otherwise interfere with any part of the Service... including security-related features or features that (a) prevent or restrict the copying or other use of Content or (b) limit the use of the Service or Content" (item 2).
- You may not "access the Service using any automated means (such as robots, botnets or scrapers)" except search engines per robots.txt or with written permission (item 3).
- Content is for "personal, non-commercial use" and you may not "publicly screen videos or stream music from the Service" (item 9).
- On downloadable software: "You are not allowed to copy, modify, distribute, sell, or lease any part of the software, or to reverse-engineer or attempt to extract the source code" (Software section).
- Reading: the terms contain no clause that bans a custom browser by name. An embedded browser that loads the unmodified `youtube.com/tv` for a person to watch personally looks like "the Service as provided". Risk points are (a) spoofing a certified-device UA to get past the device check (arguably "fraudulently engag[ing] with" the Service, item 2), (b) altering or injecting into the page (items 1, 2). This is my reading of the text, not a legal opinion.
- Separate: the [Required Minimum Functionality](https://developers.google.com/youtube/terms/required-minimum-functionality) policy is for device makers carrying YouTube apps; I did not read its text.

## Spotify

### URL, user agent: is there a TV mode?

- I found no Spotify document describing a 10-foot web UI, TV-specific URL, or TV-specific user agent. Spotify's TV support is native apps: "Many TVs offer a Spotify app. Check out the app store on your TV or set top box and search for Spotify." Source: [Spotify Support, "Spotify on TV"](https://support.spotify.com/us/article/spotify-on-tv/).
- Spotify's supported-devices page says "The app is available in some cars, TVs, gaming consoles, smart watches, and more" and lists web player browsers as "Chrome, Firefox, Edge, Opera, and Safari". It says nothing about browsers on TVs. Source: [Supported devices for Spotify](https://support.spotify.com/us/article/supported-devices-for-spotify/).
- The web player (`open.spotify.com`) is the desktop-style UI. It supports "baseline widely available versions of" Chrome, Firefox, Edge, Opera and Safari updated within the last 30 months, plus Android 6+ (Chrome 87+, Samsung Internet 17+, Firefox 115+) and iOS (Chrome 87+, Safari 15+). Source: [Spotify Support, web player help](https://support.spotify.com/us/article/web-player-help/).
- Other ways to play on a TV: "Google Chromecast, Apple AirPlay, and Now Smart Stick" ([Spotify on TV](https://support.spotify.com/us/article/spotify-on-tv/)) and Spotify Connect: "you can use one device to remotely control listening on another", with all devices on the same Wi-Fi on first connect ([Spotify Connect](https://support.spotify.com/us/article/spotify-connect/)).
- Practical conclusion for m4TV: the only documented browser-based Spotify surface is the desktop web player. A 10-foot UI around it would be m4TV's own work, e.g. via the Web Playback SDK (below).

### DRM and codecs

- The web player needs protected-content playback: the documented fix for the error "Playback of protected content is not enabled" is "you need to enable the Widevine plugin or download a media pack" (Chrome protected content IDs; Firefox "Enable DRM"; Windows N editions need the Media Feature Pack). Source: [web player help](https://support.spotify.com/us/article/web-player-help/).
- The Web Playback SDK "requires a Spotify Premium subscription (mobile only types of premium subscriptions are excluded)", is "supported by most web browsers (Chrome, Firefox, Safari and Microsoft Edge)", and "require[s] iframes to allow encrypted-media and autoplay in cases of cross origin iframes". Source: [Web Playback SDK overview](https://developer.spotify.com/documentation/web-playback-sdk). Its `initialization_error` is "Most likely due to the browser not supporting EME protection" ([SDK reference](https://developer.spotify.com/documentation/web-playback-sdk/reference)).
- Audio codecs, bitrates, and key-system levels are not published in any Spotify page I could read.

### Sign-in with remote or phone

- On TV apps: sign in with email and password, or start from the phone and "find your device using Spotify Connect", or choose "LOG IN WITH PIN" and enter the code at `www.spotify.com/pair`. Source: [Spotify on TV](https://support.spotify.com/us/article/spotify-on-tv/), "Log in to a TV app".
- Spotify Connect from a phone needs no password entry on the TV; "all devices need to be on the same WiFi" the first time ([Spotify Connect](https://support.spotify.com/us/article/spotify-connect/)).
- Web player: normal web login. I found no phone-code flow for the web player.

### Terms for custom or embedded browsers

- Spotify's [User Guidelines](https://www.spotify.com/us/legal/user-guidelines/) (incorporated into the [End User Agreement](https://www.spotify.com/us/legal/end-user-agreement/), which says "you must comply with the Spotify User Guidelines and the Platform Rules") list conduct "not permitted for any reason whatsoever in relation to the Services": "reverse-engineering, decompiling, disassembling, modifying, or creating derivative works"; "copying, reproducing, redistributing, 'ripping,' recording, transferring, performing, framing, linking to or displaying to the public"; "'crawling' or 'scraping'... or otherwise using any automated means"; "circumventing any technology used by Spotify, its licensors, or any third party, including any territorial or other content access restrictions"; "circumventing or blocking advertisements"; "deleting or altering any part of the Services or Content".
- The End User Agreement grants only "limited, non-exclusive, revocable permission to make personal, non-commercial use of the Spotify Service", and bars use "in any manner not expressly permitted by the Agreements". Third-party apps and devices "may be subject to additional terms" and Spotify "does not guarantee" compatibility. Source: [End User Agreement](https://www.spotify.com/us/legal/end-user-agreement/), sections 2 and 3 (read via the summarizing fetch tool, not directly).
- The [Spotify Developer Terms](https://developer.spotify.com/terms) restrict streaming apps: a Streaming SDA (software development application) is for "private personal use" on "Approved Devices"; "only desktop computers, laptops, netbook PCs, tablets, mobile, and such other devices that we approve in writing" (II.1); streaming SDAs must send a valid Security Code (VI.1(b)); and there is a catch-all that "Spotify may use technical measures to prevent over-usage or stop usage" (VI.2). The terms do not mention browsers or embedding, and the Widgets exemption is separate (II.9, II.14). This applies to anyone who builds against the Web API or Web Playback SDK, not necessarily a person simply opening `open.spotify.com`.
- Reading: using `open.spotify.com` unmodified in an embedded browser is the closest to "the Service as offered". Wrapping, re-skinning, injecting CSS/JS, or "framing" it into another UI is the area the User Guidelines call out. Building a Web Playback SDK player requires registering a Spotify developer app and accepting the Developer Terms, and those terms limit Approved Devices (a TV-class device is not listed unless Spotify approves it in writing). This is my reading, not a legal opinion.

## Netflix

### URL, user agent: is there a TV mode?

- I found no Netflix document describing a 10-foot web UI, a TV-specific URL, or a TV-specific UA. The help centre says: "To watch Netflix on your TV, you need a device with the Netflix app", and "On Netflix-ready TVs, the Netflix app is on the main menu or home screen." Source: [How to watch Netflix on your TV](https://help.netflix.com/en/node/33222).
- Netflix's browser requirements cover only desktop-class browsers, listed per OS with minimum version and max resolution (see next section). Source: [Netflix supported browsers and system requirements](https://help.netflix.com/en/node/30081). Its Linux section: "we can't guarantee Netflix will work on devices with a Linux OS" and support will not troubleshoot them. Same page.
- Netflix's device-partner specs (NRDP) are not public; I could not find them.

### DRM and codecs

- The browser table gives only resolution caps, not DRM or codec details: Windows Edge 118+ and Chrome 117+ up to 2160p (with the extra Ultra HD hardware requirements), Firefox 129+ and Opera 92+ up to 1080p; Mac Safari 14+ up to 2160p, Chrome 117+ up to 1080p, Edge up to 720p; ChromeOS Chrome 117+ up to 4K; Linux browsers (Chrome 117+, Firefox 129+, Edge 118+, Opera 92+) up to 1080p. Source: [supported browsers](https://help.netflix.com/en/node/30081).
- Ultra HD on Windows needs a 4K plan, 15 Mbps or more, Edge/Chrome/the Windows app, a dedicated GPU (e.g. Nvidia GeForce 1050 or newer), and "a 4K 60Hz... display with an HDCP 2.2 compatible connection". Source: [How to use Netflix on your Windows computer or tablet](https://help.netflix.com/en/node/23931).
- On Android, HD requires Widevine L1 (result summary for [Netflix Supported Devices / Android help](https://help.netflix.com/en/node/23939); I did not read this page directly).
- That DRM is Widevine or PlayReady or FairPlay by platform, and that the browser tiers are L3 vs L1/SL3000, is not stated on any page I read. The pattern in the table is consistent with it but unconfirmed.

### Sign-in with remote or phone

Source: [How to sign in to Netflix](https://help.netflix.com/en/node/311830241325668).
- QR: "On your TV, open Netflix and choose Sign in", point the phone camera at the QR code, tap the link, "Tap the Sign in to TV button. You'll be signed in to Netflix on your TV automatically."
- Code: on the TV choose Sign in, then "On your computer or mobile device, go to netflix.com/tv2" (Apple TV: `netflix.com/atv`).
- Remote: choose "Use Remote", enter email or primary-account phone number, then "Send Sign-In Link" or password. "Depending on your TV, you may not see the Send Sign-In Link option." Passwords only work with the primary account email or phone.
- If no QR/code is shown, "you'll need to use your remote to sign in." Codes time out after 15 minutes (from a search result for [Fix issues with sign-in or sign-up codes](https://help.netflix.com/en/node/529303577956964); I did not read that page directly).

### Terms for custom or embedded browsers

Source: [Netflix Terms of Use](https://help.netflix.com/legal/termsofuse?fcld=true).
- The service is accessed "on certain Internet-connected TVs, computers and other devices ('Netflix supported devices')" (preamble). The Netflix software "may solely be used for authorized access to Netflix content through Netflix supported devices" (1.10).
- Use is "for your personal, non-commercial use only" (1.4).
- You may not "circumvent, remove, alter, deactivate, degrade, block, obscure or thwart any of the content protections or other elements of the Netflix service, including the graphical user interface, any advertising or advertising features..." (1.8(iii)); "insert any code or product or manipulate the content of the Netflix service in any way" (1.8(vi)); "use any robot, spider, scraper or other automated means to access the Netflix service" (1.8(iv)); "decompile, reverse engineer or disassemble" (1.8(v)); data mining (1.8(vii)).
- Reading: a stock desktop-class browser on the supported-browsers list is the permitted path. Wrapping Netflix in a custom shell that rewrites the UI or injects code touches 1.8(iii) and 1.8(vi). The terms do not define "supported device" beyond the web page lists, and the TV-specific path is the certified Netflix app.

## Unverified / gaps

- No primary source found for: any Spotify or Netflix TV-mode URL or UA (probably none exist; absence of documentation is not proof); Spotify audio codecs; Netflix DRM system names per browser; Netflix codec lists.
- The YouTube CE spec I read is the 2020 revision, from the Wayback Machine (the original URL on developers.google.com 404s). Newer revisions (2027/2028 pages) are partner-gated, so current Widevine level, AV1 and VP9 requirements may differ.
- Whether `youtube.com/tv` fully works (playback, sign-in) in a non-certified embedded browser with a TV-style UA is unverified. I only confirmed that the server returns the full app markup to TV-style UAs and an "app store" interstitial to a desktop Chrome UA; the page returns different payloads, not proof of playback.
- Whether YouTube enforces the certification-signature URL parameter on non-certified clients is not documented.
- Several pages were read via a summarizing fetch tool rather than raw HTML (Spotify End User Agreement, Developer Terms, most help pages); quoted text for the User Guidelines was verified from raw HTML. Terms and help pages are living documents and may change.
- The Spotify Developer Terms page was fetched once; I did not check the separate Developer Policy.
- The terms readings are plain-language interpretation, not legal advice.
