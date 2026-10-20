# What may a published m4TV image legally include?

Research for GitHub issue m4rocks/m4TV#7 (parent #2, blocks #14). Retrieved 2026-10-20.

**This is research, not legal advice.** Points marked **[LAWYER]** need a qualified read before m4TV is published. Points marked **[UNVERIFIED]** could not be confirmed from a primary source.

Method note: pages were read through a fetch tool that returns summaries and short quotes of each page. Quotes below are short and as returned. Section numbers come from that output. Re-check anything load-bearing against the live page before relying on it.

## Short answers

1. **Shipping the Widevine CDM in the image: do not.** No public Google document grants a right to redistribute the CDM binary to the public. Google says "A license agreement is required for the use of Widevine products or services" ([Widevine overview](https://developers.google.com/widevine/drm/overview)). The only licence text Google publishes is for a mobile-OS licence, and it says it is "not for any other purpose" ([Widevine mobile licence](https://developers.google.com/widevine/contact/widevine-mobile-license-use)). Chromium itself does not bundle the CDM (see Q1).
2. **Downloading the CDM at first run on the user's machine** is the pattern that Firefox, Chrome and the castLabs Electron build use. Whether m4TV is allowed to do the same without its own Widevine agreement is **not stated in any public primary source**. **[LAWYER]** plus a direct question to widevine@google.com.
3. **Legitimate Widevine integration** means a signed Widevine agreement. Repository access is "contingent upon signing of the Widevine Master License Agreement" and requires a corporate email ([Widevine access](https://developers.google.com/widevine/access)). The public pages describe the process for device makers, chipset vendors and content providers. They say nothing about hobby or open-source projects.
4. **Engine licences** (Chromium, CEF, WPE) all permit redistribution of the open-source parts. The obligations are notice-keeping for BSD code and source and relinking duties for LGPL parts. None of them licenses the CDM, and none removes the codec patent question.
5. **Logos and names:** YouTube, Netflix and Spotify all forbid using their marks in your product's name or in ways that imply endorsement. YouTube and Spotify publish concrete rules. Netflix's brand rules were not retrievable. A small "launcher tile" with each service's logo is the risky part.
6. **User-agent spoofing:** no primary source says that sending a different User-Agent string is itself illegal. It is discouraged by the HTTP spec. It can collide with each service's "no circumvention" terms, and it is the thing most likely to get accounts or devices blocked. **[LAWYER]**

## Q1. Can a public image ship or download the Widevine CDM, and on what terms?

### What is public

- Google says Widevine does not charge for its products, but "A license agreement is required for the use of Widevine products or services." ([Widevine overview](https://developers.google.com/widevine/drm/overview))
- Widevine's own site says the DRM comes "without licensing fees or required participation in the CWIP training program" ([widevine.com](https://www.widevine.com/)). That page is silent on CDM distribution and on who may receive the CDM.
- Device manufacturers must complete "Completion of legal agreements" before integration, followed by library and SDK exchange and testing against "the Widevine robustness rules" ([Widevine DRM solution page](https://www.widevine.com/solutions/widevine-drm)).
- The overview says "The Widevine client is natively embedded into a device platform unless otherwise noted." For device integration it says "please contact us" ([Widevine overview](https://developers.google.com/widevine/drm/overview)). The support page covers support for "hardware party (device manufacturer or chipset vendor)", "certified Widevine partner" and "existing Widevine licensee". It does not say whether individuals or open-source projects can obtain a licence ([Widevine support](https://developers.google.com/widevine/contact/support)).

### The only published licence text, and why it does not cover m4TV

The "Widevine License Agreement" pages are scoped to Google's mobile and XR licence agreements. The header says it applies "only for the purposes of the Mobile License Agreement" "and not for any other purpose", and "If you would like to enter into a Widevine agreement for a different purpose, please contact widevine@google.com" ([mobile](https://developers.google.com/widevine/contact/widevine-mobile-license-use)). Useful as an indication of the typical terms, not as a grant to m4TV:

- Grant is "worldwide, non-exclusive, non-transferable", "non-sublicensable" during the Term (s2.1).
- Redistribution is only to "distribute the Client Software to users through one or more tiers", and sublicensing to distributors is limited to those rights and on terms "no less protective of Google's rights" (s2.1.1(c),(d)).
- Licensee may not "reproduce, modify, translate, enhance, decompile, disassemble, reverse engineer, or create derivative works" or "circumvent or disable any technological features or measures" (s3.1.1).
- Exhibit A sets robustness requirements for devices (secure boot loader, authenticated code, tamper-resistant key protection, no keys or clear content on user-accessible buses, self-checking, protected outputs). It applies to platforms the licensee makes. A general-purpose Linux image with a user-modifiable OS is unlikely to meet such rules, but that is my reading of the exhibit, not a Google statement. **[LAWYER]** / ask Widevine.

### What the engines actually do

- **Chromium does not bundle the CDM.** In `widevine.gni`, `enable_widevine` is on by default only for Chrome-branded builds, Chrome for Testing and Android ("Can be optionally enabled in Chromium on non-Android platforms"). `bundle_widevine_cdm` is true only for "is_chrome_branded || is_chrome_for_testing_branded" ([widevine.gni](https://chromium.googlesource.com/chromium/src/+/main/third_party/widevine/cdm/widevine.gni)).
- **Chrome/Chromium fetch the CDM at runtime** through the component updater (extension id `oimompecagnajdejgnnjijobebaeigek`; on Linux it is loaded at startup) ([widevine_cdm_component_installer.cc](https://raw.githubusercontent.com/chromium/chromium/main/chrome/browser/component_updater/widevine_cdm_component_installer.cc)). When the CDM is bundled, the build copies a `LICENSE` file next to it ([BUILD.gn](https://chromium.googlesource.com/chromium/src/+/main/third_party/widevine/cdm/BUILD.gn)). I could not retrieve the text of that licence. **[UNVERIFIED]**
- **Firefox does not ship it either.** Its fallback source list `gmp-sources/widevinecdm.json` points `fileUrl` at `edgedl.me.gvt1.com` (mirrors on `www.google.com`) with SHA-512 hashes ([widevinecdm.json](https://raw.githubusercontent.com/mozilla/gecko-dev/master/toolkit/content/gmp-sources/widevinecdm.json)). The code installs it on demand, and Mozilla's bug on Linux says "We don't want the Widevine CDM to be downloaded by default on Linux, as the CDM is proprietary software" and proposes asking the user first ([Bug 1289634](https://bugzilla.mozilla.org/show_bug.cgi?id=1289634)). This shows an opt-in download model. It does not show what Mozilla's agreement with Google says. **[UNVERIFIED]**
- **Electron (castLabs build)** installs the CDM "on first launch" and signs builds for development only. For production it points to a paid "EVS" signing service, noting "Previously a license agreement with Google Widevine was required to get your own VMP signing certificate" ([castlabs/electron-releases](https://github.com/castlabs/electron-releases)). (My inference, not sourced: VMP signing is what licence-server operators may require for production clients.) This is a third-party commercial route and a possible fallback if m4TV needs a signed Chromium-family client. castLabs's own terms were not reviewed.
- **Chrome's own terms:** the Chrome Additional Terms page that was retrievable covers the AVC notice and says the terms apply to the executable (compiled) version of Chrome ([Chrome terms](https://www.google.com/chrome/terms/)). It has no Widevine or redistribution text in the part I could read. If m4TV took the CDM out of a Chrome package, this would be a use outside the Chrome terms. **[LAWYER]**

### What this means for m4TV

| Option | Status from primary sources |
| --- | --- |
| Put the CDM file in the published image | No public grant found. Treat as not allowed unless Widevine agrees in writing. |
| First-run download from Google's CDN, user opt-in (Firefox / Chrome style) | Practiced by Mozilla and Google. Third-party rights to do this are not documented. **[LAWYER]** and ask widevine@google.com. |
| Scrape or copy the CDM from a Chrome install or an Android image | Outside any published grant. Not recommended. |
| Sign a Widevine agreement | Possible in principle. Public pages target device makers and content providers. Corporate email required. |
| Ship a build that does not contain or fetch the CDM, user supplies it | Reduces redistribution exposure. Does not resolve user-side terms. **[LAWYER]** |

Technical note, not legal: Google's page lists "Chromium Embedded Framework (CEF) / Electron" as supported ("Yes"), "Chromium Browser" "Yes", "Firefox Browser" "Yes". There is no WPE row ([overview](https://developers.google.com/widevine/drm/overview)). The overview does not discuss security levels, so what a Linux desktop-class client can unlock (resolution caps) must come from each service. Netflix's help page lists Linux browsers (Chrome 117+, Firefox 129+, Edge 118+, Opera 92+) up to 1080p and says it "can't guarantee Netflix will work on devices with a Linux OS" and support will not troubleshoot Linux ([Netflix browsers](https://help.netflix.com/en/node/23742)). Sources conflict: older English and Italian snapshots seen in search results capped Linux at 720p with other minimum versions, so the 1080p figure is **[UNVERIFIED]** until re-read live.

## Q2. What does a legitimate Widevine integration require?

From the public pages:

1. A Widevine licence agreement. Repository access (client and server code, SDKs) is "contingent upon signing of the Widevine Master License Agreement", needs "company or corporate email", and "Access to each repository is evaluated and approved separately" with "Additional technical discussion" possible. Personal email accounts are rejected ([access](https://developers.google.com/widevine/access)).
2. For devices: legal agreements, documentation, technical review of robustness rules, library exchange, "keybox fulfillment (as needed)", then integration testing ([solution page](https://www.widevine.com/solutions/widevine-drm)). Level 1 devices also need "the appropriate OEMCrypto library from your chipset vendor" ([access](https://developers.google.com/widevine/access)).
3. Content-side (not m4TV's role): the licence server is partner-run. The client "does not communicate directly with the Widevine License Service, under any circumstances" ([overview](https://developers.google.com/widevine/drm/overview)). Each streaming service (Netflix, Spotify, YouTube) runs its own and decides which clients it serves.
4. For a browser-style integration the public docs do not describe a distinct process. Browsers named in the support matrix are Google's or Google-partnered. **[UNVERIFIED]** whether a small open-source distribution can qualify.

## Q3. Licence obligations of the Engine candidates

### Chromium

- Chromium source is under the BSD 3-clause licence: keep the notice, conditions and disclaimer in source and in binary documentation, and do not use Google's or contributors' names to endorse derived products without written permission ([Chromium LICENSE](https://raw.githubusercontent.com/chromium/chromium/main/LICENSE)).
- Chromium bundles many third-party components with their own licences (listed in `chrome://credits`; Chrome says "Most source code for Chrome is available free of charge under open source software license agreements at chrome://credits" ([Chrome terms](https://www.google.com/chrome/terms/))). A Chromium-based image must ship the credits page. FFmpeg, which Chromium uses for media, is LGPL when built without `--enable-gpl`/`--enable-nonfree`. FFmpeg's checklist: use dynamic linking, "Distribute the source code of FFmpeg, no matter if you modified it or not", avoid GPL libs such as libx264 ([FFmpeg legal](https://ffmpeg.org/legal.html)). I did not verify how Chromium's bundled FFmpeg meets this. **[LAWYER]**
- **Codec patents:** H.264/AAC are patent-encumbered. Chrome's terms carry the AVC notice (personal, non-commercial-paid-use; "NO LICENSE IS GRANTED OR SHALL BE IMPLIED FOR ANY OTHER USE") ([Chrome terms](https://www.google.com/chrome/terms/)). FFmpeg warns that commercial products may face licensing fee demands from patent owners ([FFmpeg legal](https://ffmpeg.org/legal.html)). Whether a free public image needs patent licences is a **[LAWYER]** question and applies to every engine candidate.
- Branding: "Google Chrome" is not Chromium. Do not call a Chromium build Chrome. I did not retrieve Google's Chromium/Chrome trademark policy. **[UNVERIFIED]**

### CEF

- CEF is BSD-licensed ([CEF README](https://raw.githubusercontent.com/chromiumembedded/cef/master/README.md)). The LICENSE is a three-clause BSD text with a no-endorsement clause ([LICENSE.txt](https://github.com/chromiumembedded/cef/blob/master/LICENSE.txt)). CEF asks you to "include the license text somewhere in your application's distribution", and exposes `about:license` and `about:credits` ([General Usage](https://github.com/chromiumembedded/cef/blob/master/docs/general_usage.md)).
- CEF inherits Chromium's third-party licences and codec issues above. Official CEF builds do not mention Widevine in the pages I could read, so CDM handling is the integrator's problem. Google's matrix lists CEF as supported but that is a technical statement, not a distribution licence. The CEF Widevine loader source and wiki pages I tried returned 404; **[UNVERIFIED]** current CEF behaviour for Widevine.

### WPE WebKit

- WPE "is published under a mix of LGPLv2 and BSD licenses". libwpe and WPEBackend-fdo are BSD-2-Clause and Cog is MIT ([WPE FAQ](https://wpewebkit.org/about/faq.html)).
- LGPL duties (WebKit licensing page): redistribute the library with complete source, keep notices, include the licence, mark changes, and for works that link against it give prominent notice, supply the licence, and provide source or materials that allow relinking ([WebKit licensing](https://webkit.org/licensing-webkit/)). That page states the LGPL as "GNU LIBRARY GENERAL PUBLIC LICENSE" v2 (June 1991). BSD parts need notice reproduction in binary documentation.
- **DRM:** WebKit contains EME code, but "you will need a license agreement with DRM CDM providers to access it" because "this part is not open source." Listed routes: Thunder OCDM plugin, a Thunder-compatible API for your DRM system, or a new CDM backend ([WPE FAQ](https://wpewebkit.org/about/faq.html)). The FAQ does not mention Widevine. An Igalia developer blog (secondary source, around 2020) says the Widevine path in WPE goes through Thunder and that the Widevine sources need credentials because they are closed ([Igalia](https://blogs.igalia.com/xrcalvar/category/planets/planet-webkitgtk/)). **[UNVERIFIED]** for current state.
- Consequence: for WPE a Widevine agreement is a precondition, not an option, since there is no public CDM download path equivalent to Chrome's component updater that I could confirm.

## Q4. Service trademarks and logos

Only the owner's own text is cited. Rules differ by whether m4TV (a) merely opens the service's website in a browser, (b) uses the service's developer API/SDK, or (c) displays the logo.

### YouTube

- Terms: "Content" includes "branding (including trade names, trademarks, service marks, or logos)", and the Terms say use of the Service does not give you rights in it ([YouTube Terms](https://www.youtube.com/t/terms?hl=en&gl=US); the Romanian page was also checked and agrees).
- Developer branding guidelines (apply to API clients): "You must never use the YouTube name or any abbreviation, acronym, or variant of the word YouTube" "in conjunction with the overall name of your application". The logo "must not be altered or partially covered", colours cannot be changed, and "Any YouTube logo used within an application must link back to YouTube content". YouTube "may request that you modify your app or usage or that you cease to use the logos" ([YouTube branding](https://developers.google.com/youtube/branding)).
- API Services Developer Policies: forbid modifying, blocking or building on the player, background playback, blocking ads, and using "any technology other than YouTube API Services to access or retrieve API Data" (III.I.6, III.I.9, III.I.5, III.I.14), and "you must not use any other means to mask or misrepresent your API Client's access" (III.D.1.b) ([policies](https://developers.google.com/youtube/terms/developer-policies)). These bind only if m4TV uses the API. Opening youtube.com in an embedded browser is not an API client. Whether the policies apply to a TV shell is **[LAWYER]**.
- `brand.youtube` returned only image filenames. I could not read the general YouTube brand-use rules. **[UNVERIFIED]** (search snippets suggest requests go through a form and merch is refused; secondary).

### Netflix

- Legal Notices: "Netflix, the N Logo and its sonic Tudum ident are trademarks of Netflix, Inc." "Unless you have our permission, do not use the Netflix marks as your own", nor "in any manner that implies sponsorship or endorsement by Netflix". Goods with the name or logo are reserved for licensees ([Netflix Legal Notices](https://help.netflix.com/legal/notices)). The page does not say how to ask for permission.
- Terms of Use (s1.8): users may not "circumvent, remove, alter, deactivate, degrade, block, obscure or thwart any of the content protections", nor use "any robot, spider, scraper or other automated means"; the licence is a limited right to access the service, and s1.10 limits software to "authorized access to Netflix content through Netflix supported devices" ([Netflix Terms](https://help.netflix.com/legal/termsofuse)).
- `brand.netflix.com` guidelines: page returned only a title. **[UNVERIFIED]** Media Center terms (press assets) say assets must be used unmodified, but those cover the press kit, not product use ([Media terms](https://help.netflix.com/legal/media-terms-and-conditions), via search summary only).
- Netflix's partner/device programme: public pages were not retrievable. The material found was vendor press releases (secondary). **[UNVERIFIED]**

### Spotify

- Design guidelines: "The app name should not include 'Spotify' or be similar to 'Spotify' in sound or spelling". "Don't incorporate Spotify's trademarks, in whole or in part, in the name of your company". Do not use another brand beside it or modify the logo. "It shouldn't imply endorsement by Spotify, but suggesting to users that it is 'for Spotify' is acceptable". The green logo only on black or white ([Spotify design guidelines](https://developer.spotify.com/documentation/design)).
- Developer Terms (apply if you use the Spotify Platform/SDK): marks usable "solely to promote your use" (III.1.c), not "as part of the name of your company or service" (IV.2.c.ii), "You may not suggest a partnership with, sponsorship by, or endorsement by Spotify" (IX.7), no circumventing "geographical restrictions" or "stream ripping", and "Distribute the Spotify Platform only in binary form as part of an SDA" ([Spotify Developer Terms](https://developer.spotify.com/terms)).
- Developer Policy: commercial use of streaming apps is not permitted ("commercial uses are not permitted for SDAs"), and "Do not build products or services that mimic, or replicate or attempt to replace a core user experience of Spotify" without permission ([Developer Policy](https://developer.spotify.com/policy)). The Web Playback SDK page says it "requires a Spotify Premium subscription", and "must not be used in commercial projects without Spotify's prior written approval" ([Web Playback SDK](https://developer.spotify.com/documentation/web-playback-sdk)). Whether m4TV being free and open-source counts as non-commercial is a **[LAWYER]** question.
- Consumer Terms: limited to "personal, non-commercial use of the Spotify Service" ([Spotify Terms](https://www.spotify.com/us/legal/end-user-agreement/)). User Guidelines forbid "circumventing any technology used by Spotify", "circumventing or blocking advertisements", automated means and reverse-engineering ([User Guidelines](https://www.spotify.com/us/legal/user-guidelines/)).

### Practical reading (non-binding)

- Name the product without service names; do not use their logos as m4TV branding.
- If a launcher tile for a service is wanted, treat it as nominative reference to the user's own account and expect the owner's brand rules, not a default right. **[LAWYER]**.
- Prefer the service's own web UI over scraped or reimplemented clients, since Spotify's policy bars replicating its core experience and YouTube's bars altering the player and ads.

## Q5. User-agent spoofing

- HTTP spec: a sender "SHOULD limit generated product identifiers to what is necessary to identify the product"; servers use the header for "analytics", to "work around or tailor responses to avoid particular user agent limitations", and interoperability triage. Implementations are encouraged not to use other implementations' product tokens to claim compatibility ([RFC 9110 s10.1.5](https://www.rfc-editor.org/rfc/rfc9110.html)). The spec adds that using another implementation's token "circumvents the purpose of the field". That wording is guidance, not a prohibition; the tool returned the last quote as a fragment, so re-read s10.1.5 before quoting it.
- Services' contracts do not mention user agents in the text retrieved. They contain general anti-circumvention clauses: YouTube ("circumvent, disable, fraudulently engage with, or otherwise interfere with any part of the Service") ([YouTube Terms](https://www.youtube.com/t/terms?hl=en&gl=US)); Netflix s1.8 content-protection clause ([Netflix Terms](https://help.netflix.com/legal/termsofuse)); Spotify "circumventing any technology used by Spotify" ([User Guidelines](https://www.spotify.com/us/legal/user-guidelines/)); YouTube API III.D.1.b "mask or misrepresent your API Client's access". Whether a spoofed UA that unlocks a TV-only experience counts is a question of how a court reads those clauses. **[LAWYER]**
- Statutory: US 17 USC 1201(a)(1)(A) says "No person shall circumvent a technological measure that effectively controls access to a work" and defines circumvention to include "avoid, bypass, remove, deactivate, or impair a technological measure" ([17 USC 1201](https://www.law.cornell.edu/uscode/text/17/1201)). Changing the UA string is not decryption, but this is an argument a rightsholder could try. Bypassing Widevine itself clearly is within this territory. EU and national law were not researched. **[LAWYER]**
- Service behaviour: Google's own help (seen only via a search-result summary, not fetched) says it turns off services on devices that cannot meet security standards, and that if a TV has a browser it may still be possible to use YouTube through youtube.com ([YouTube Help](https://support.google.com/youtube/answer/6098135)). The 2019 retirement of the youtube.com/tv "Leanback" interface was reported as tied to uncertified devices; this is from press coverage (Tubefilter, Variety), not Google. **[UNVERIFIED]**
- Practical reading: use an honest UA string for the engine. If a service needs a specific one for a feature, make that an explicit per-site user setting, off by default, rather than a shipped default, and do not market it as a way around a service's device or DRM checks.

## What I could not verify

- The Widevine CDM's own licence text (the `LICENSE` bundled with the CDM) and whether any agreement lets third parties redistribute or fetch the CDM. Needs Widevine (widevine@google.com) and a lawyer.
- Mozilla's and castLabs's arrangements with Google (inferred only from what they do).
- Current CEF Widevine handling (CEF source/wiki URLs 404ed).
- Current WPE Widevine support (WPE FAQ silent; Igalia source is secondary and about 2020).
- Netflix brand-guidelines page and Netflix device-partner requirements; YouTube general brand-use rules at brand.youtube.
- Netflix Linux resolution table (single summarised fetch), YouTube Leanback retirement reason (press only).
- Chromium/Google trademark policy for Chromium builds, and how Chromium's bundled FFmpeg meets LGPL.
- EU law on circumvention; patent-licence need for H.264/AAC in a free image.
