import { expect, test } from "vitest";

// ADR 0005: the look is declared once, in design/tokens.json. Tailwind reads it, so a class is
// already a token; what this catches is a value written past it.
const sources = import.meta.glob(["./**/*.{ts,tsx,css}", "!./**/*.test.ts", "!./protocol/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;
const page = import.meta.glob("../index.html", { query: "?raw", import: "default", eager: true }) as Record<string, string>;

const hardCoded = [
  { what: "a hex colour", pattern: /#[0-9a-fA-F]{3,8}\b/ },
  { what: "a Tailwind arbitrary value", pattern: /\b[a-z-]+-\[/ },
];

test("no source file writes a colour or an arbitrary value instead of using a token", () => {
  const files = { ...sources, ...page };
  expect(Object.keys(files).length).toBeGreaterThan(5);
  const found = Object.entries(files).flatMap(([file, text]) =>
    hardCoded.filter(({ pattern }) => pattern.test(text)).map(({ what }) => `${file}: ${what}`),
  );
  expect(found).toEqual([]);
});
