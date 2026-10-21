import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// ADR 0005: design/tokens.json is the one place the look is defined; the Shell reads it too.
const tokens = JSON.parse(
  readFileSync(fileURLToPath(new URL("../../design/tokens.json", import.meta.url)), "utf8"),
);
const px = (n) => `${n}px`;
const rem = (n) => `${n / 16}rem`;
const mapValues = (object, f) => Object.fromEntries(Object.entries(object).map(([k, v]) => [k, f(v)]));
const variables = (palette) =>
  Object.fromEntries(Object.entries(palette).map(([name, value]) => [`--color-${name}`, value]));

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    // Only the tokens' colours exist: a default palette class (bg-gray-200) is not a colour here.
    colors: {
      transparent: "transparent",
      current: "currentColor",
      ...Object.fromEntries(Object.keys(tokens.color.dark).map((name) => [name, `var(--color-${name})`])),
    },
    extend: {
      borderRadius: {
        DEFAULT: px(tokens.radius.small),
        lg: px(tokens.radius.medium),
        xl: px(tokens.radius.large),
      },
      spacing: mapValues(tokens.spacing, rem),
      fontFamily: { sans: ["Inter", "system-ui", "sans-serif"] },
      fontSize: mapValues(tokens.type, ([size, line]) => [rem(size), rem(line)]),
      transitionDuration: mapValues(tokens.motion, (ms) => `${ms}ms`),
    },
  },
  plugins: [
    // The Remote follows the phone's own light or dark preference, not the TV's Theme.
    ({ addBase }) =>
      addBase({
        ":root": variables(tokens.color.light),
        "@media (prefers-color-scheme: dark)": { ":root": variables(tokens.color.dark) },
      }),
  ],
};
