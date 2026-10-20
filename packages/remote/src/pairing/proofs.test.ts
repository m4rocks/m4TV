import { expect, test } from "vitest";
import { proof } from "./proofs";

// Worked with Python's hmac module, and asserted by the Rust side too, so the two cannot drift.
const secret = "000102030405060708090a0b0c0d0e0f";

test("proofs match the Remote server's", () => {
  expect(proof(secret, "phone", "c0ffee", "beef")).toBe(
    "38f3841968815fc649d7f027c8eb4450fd38974e32c89e1ccc6f54b39209889a",
  );
  expect(proof(secret, "tv", "c0ffee", "beef")).toBe(
    "e4b4b823435e6476eed9434ed116ce6f4e2e29573a873e028442b499d81c5ffd",
  );
  expect(proof(secret, "token", "c0ffee", "beef")).toBe(
    "f28bee0ce3afed378f068ad9586cac825a8f4dbd37b69274153e3f0b1e38cceb",
  );
});
