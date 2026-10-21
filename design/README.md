# Design

The Shell's look is declared once, here (ADR 0005).

- `tokens.json`: colours per theme, radii, spacing, type scale, motion times and the Safe area. `packages/remote/tailwind.config.js` reads it, and `crates/shell/build.rs` turns it into the constants in `m4tv_shell::tokens`. Tests fail when either side writes a value past it.
- `fonts/`: Inter 4.1 Regular and Bold (TTF, for the Shell), from <https://github.com/rsms/inter/releases/tag/v4.1> (`Inter-4.1.zip`, sha256 `9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e`, files under `extras/ttf/`), under the SIL Open Font License 1.1 (`OFL.txt`).
- `public/fonts/`: the same two faces as Latin-subset WOFF2, which Vite copies into the Remote, plus the licence text so it is served with them. Made from `fonts/` with fontTools:

  ```
  pyftsubset fonts/Inter-Regular.ttf --unicodes="U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+2000-206F,U+2074,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,U+FFFD" --layout-features='*' --flavor=woff2 --output-file=public/fonts/Inter-Regular-latin.woff2
  ```

  (the same for Bold). `--layout-features='*'` keeps the tabular figures the clock and the PIN use.
