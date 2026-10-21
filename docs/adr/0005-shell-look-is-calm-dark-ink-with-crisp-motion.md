# The Shell looks calm, content-first and neutral, with crisp motion that fits a Pi 4

The Shell's identity is a calm, content-first interface: near-black surfaces, one neutral "ink" accent, and the Apps' own logos as the only colour. Motion is short and ease-out. Every effect has to fit the Pi 4's frame budget on both boards (Story 11), so what a Pi 4 cannot afford is cut for the Pi 5 too. The Shell and the Remote share one set of design tokens. This records the decisions; the build tickets prototype them.

## Decisions

- **Identity**: calm, dark-first and content-first. No brand colour of our own: the logos are the colour, so a neutral palette never fights YouTube red, Netflix red or Spotify green.
- **Themes**: Dark (the default) and Light, a Theme Setting. There is no Auto. Overlays follow the theme and sit on a panel that guarantees contrast over any video.
- **Accent**: "ink", the maximum-contrast neutral against the surface: white on Dark, near-black on Light. It carries focus, selection and the volume bar. One muted state colour is kept for warnings and errors (the Setup hotspot failure reason, no network).
- **Typeface**: Inter, under the SIL Open Font License 1.1, bundled as a separate font asset with its licence text. It is neutral, legible at distance and has tabular figures for the clock and the PIN. The OFL permits shipping it with an Apache-2.0 project. The Shell loads it from a TTF; the Remote is served a Latin-subset WOFF2 of the same face. SF is not licensed for use off Apple platforms.
- **Safe area**: a fixed 5% margin for all text and Tiles; backgrounds bleed to the edge. There is no overscan Setting in v1.
- **Home**: a Hero above one horizontal row of Tiles. The Hero holds a clock (24-hour by default, a Setting; hidden until the OS seam reports a trustworthy time, because the Pi has no RTC) and a one-line status (network, paired phone). It holds nothing the Shell lacks data for: no now-playing, no recommendations.
- **Tiles**: every Tile is the same neutral surface with the App's official logo centred, using the service's own light or dark variant for the theme, never recoloured by us. Settings gets a plain glyph. All artwork is bundled, so there is no loading state. The name shows under the Tile only while it is focused.
- **Focus**: the focused Tile scales up about 8% with a soft shadow and a thin ink outline. No glow and no parallax. A focused Settings row gets the same outline and a slight lift.
- **Motion**: ease-out, 150 to 250 ms, no overshoot. Launching an App grows the focused Tile to fill the screen (about 200 ms), holds its logo on a calm backdrop until the Engine delivers its first frame, then crossfades (about 200 ms) to the App. Leaving reverses it. A Reduce motion Setting (default off) turns focus scale and the launch transition into short fades.
- **Overlays**: a panel in the theme's surface colour at about 90% opacity. There is no backdrop blur in v1 (see below).
- **Settings screen**: two panes, categories on the left and rows on the right. Up and Down move within a pane; OK or Right on a category enters its rows; Left and Right on a row change the value, as they do today; Back returns to the categories, and a second Back leaves Settings. Each setting declares a `category` in the shared declaration, so the Shell's panes and the Remote's grouped lists come from the same field.
- **First boot**: the Setup hotspot takes over the whole screen with a large Wi-Fi QR on a white card, in both themes, with two numbered steps. The handover to the new address reuses that layout. The Pairing PIN is a normal Overlay, not a screen.
- **Remote**: the Remote is part of this design. It uses the shared tokens (ink palette, radii, spacing, type scale), follows the phone's own light/dark preference rather than the TV's Theme (the TV Theme is just a TV setting shown as a row), uses Inter, and limits motion to press feedback and `prefers-reduced-motion`. It has none of the Shell's focus scale or launch transition.
- **Tokens**: one `design/tokens.json` (colours per theme, radii, spacing, type scale, motion durations) read by `tailwind.config.js` and by the Shell at build time, so the look is declared once, as Settings and protocol types already are. A test fails when either side hard-codes a value that is not a token.
- **Judging the Pi 4**: the Dev host shows the look, never the performance. A frame-time readout turned on by an environment variable (not a Setting, so the Remote never shows it) that also writes p50, p95 and p99 frame times and dropped video frames to a log file, and a fixed checklist run by hand on the Pi 4 and Pi 5 at milestones (focus move, launch, an Overlay over 1080p30 video): after the Home, launch and Overlay tickets land, and before each release. The numbers go in the table and bars below.

## Effect budget

None of these costs is measured yet; the Pi 4 numbers come from the frame-path spike (#32) and the judging step above. Until then each effect is marked with what happens to it if it does not fit.

| Effect | Cost | Within the Pi 4 budget |
| --- | --- | --- |
| Focus scale, soft shadow, outline | One transform and one shadow on a focus move, nothing while idle | Kept. If it drops frames: drop the shadow, keep scale and outline, on both boards |
| Launch grow and crossfade | One transform, then one opacity animation, while CEF is loading | Kept. Falls back to the Reduce motion fades; the fades are the floor |
| Overlay panel at about 90% opacity | One alpha-blended quad over the App frame | Kept |
| Hero clock | Redraws once a minute | Kept; the Shell never redraws idle frames for it |
| Backdrop blur on Overlays | A render pass over the live App frame | **Cut.** gpui-pre 0.3.9 has no backdrop blur (only shadow blur and a Windows-only window material), so it would mean a custom render pass in a gpui fork. It returns, on both boards, only if the Pi 4 meets every bar below with at least 25% frame time spare at p95 while an Overlay is up (#60) |
| Glow in the App's colour, parallax or tilt | Per-frame blur or layered artwork | **Cut** |

## Pass bars

Written before the first measurement, so the spike (#32) and the checklist (#59) give pass or fail. Each is measured on a Pi 4 with an Overlay over 1080p30 video, and a Pi 5 must meet the same bars:

- At most 1% dropped video frames over 5 minutes.
- The idle Shell averages at least 58 fps.
- The Shell holds at least 30 fps while video plays.
- At least 1 GB of RAM is free with the three Apps loaded.

If the Pi 4 misses, the remedies go in this order: the 30 fps Shell fallback first (a setting, no fork), and the dmabuf fork of `gpui-pre-wgpu` only if video frames still drop. Video stays 1080p30 and both boards stay identical. These numbers are my starting guesses and may change once a Pi has run them; changing a bar is an edit to this section.

## Considered Options

- **Brand-coloured Tiles**: vivid, but three loud brand colours fight the calm identity and lean harder on the branding risk accepted in #14. Rejected in favour of neutral Tiles with the official logo.
- **Backdrop blur on Overlays (the Apple look)**: wanted, and kept as a stretch, but it is the likely budget breaker and needs a gpui fork. Rejected for v1.
- **A colour accent (indigo, teal, amber)**: gives identity, but sits close to a service brand colour. Rejected for a neutral ink accent.
- **System font in the Shell**: nothing to ship, but the Pi image and the Dev host would not match, so the look could not be judged on the Dev host.
- **Dark only**: one palette to test and the cheapest. Rejected: Light is a Setting, which doubles the visual checks and adds Overlay contrast cases, accepted for daylight rooms.
- **Single column Settings**: matches today's list and is simpler. Rejected for two panes, which needs a `category` on every setting and a second focus level.

## Consequences

- The shared settings declaration gains a `category`, and new Settings: Theme, Reduce motion and the 24-hour clock. The Remote picks them up through the declaration with no extra Remote code.
- Light doubles the visual QA: every screen, every logo variant and every Overlay is checked in both themes.
- The Shell embeds two Inter files (TTF for gpui, WOFF2 for the Remote) from one licensed source, and the Remote carries a font payload on the plain-HTTP Setup hotspot path, kept small by subsetting.
- The ink accent has no colour to tell states apart, so state (errors, warnings) needs the one muted state colour and never relies on colour alone.
- Three Apps times two logo variants means the logo assets track whatever the services publish; a service that offers no usable variant for a theme needs a decision at that point.
- The Remote's current Tailwind defaults (`bg-blue-600`, `bg-gray-200`) are replaced by tokens.
