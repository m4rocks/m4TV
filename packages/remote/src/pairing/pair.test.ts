import { expect, test } from "vitest";
import { pairByPin, pairBySecret, pairOnHotspot, parseLink, type Post } from "./pair";
import { proof } from "./proofs";

const secret = "000102030405060708090a0b0c0d0e0f";

/** A TV that knows `secret`, as the Remote server does (crates/shell/src/remote/pairing.rs). */
function tv(knows = secret): Post {
  return async (path, body: any) => {
    if (path === "/api/pair/challenge") return { status: 200, body: { challenge: "c0ffee" } };
    if (path === "/api/pair/prove") {
      const ok = body.proof === proof(secret, "phone", body.challenge, body.nonce);
      return ok
        ? { status: 200, body: { proof: proof(knows, "tv", body.challenge, body.nonce) } }
        : { status: 403, body: { error: "wrong secret" } };
    }
    if (path === "/api/pair") {
      return body.pin === "1234"
        ? { status: 200, body: { token: "t0ken" } }
        : { status: 403, body: { error: "wrong PIN" } };
    }
    return { status: 404, body: {} };
  };
}

test("the QR link's fragment gives the secret and the fingerprint to compare", () => {
  expect(parseLink("#secret=abcd&fp=AA:BB")).toEqual({ secret: "abcd", fp: "AA:BB" });
  expect(parseLink("")).toBeNull();
  expect(parseLink("#fp=AA:BB")).toBeNull();
});

test("pairing by the QR secret yields the token the TV worked out", async () => {
  const token = await pairBySecret(tv(), secret, "Pixel");
  // The token never travels: it is the phone's own HMAC for the challenge and nonce it sent.
  expect(token).toMatch(/^[0-9a-f]{64}$/);
});

test("a TV that does not know the secret is not trusted", async () => {
  await expect(pairBySecret(tv("ff".repeat(16)), secret, "Pixel")).rejects.toThrow(
    "did not prove",
  );
});

test("a refused proof surfaces the TV's reason", async () => {
  await expect(pairBySecret(tv(), "11".repeat(16), "Pixel")).rejects.toThrow("wrong secret");
});

test("pairing by PIN yields the token, and a wrong PIN says so", async () => {
  expect(await pairByPin(tv(), "1234", "Pixel")).toBe("t0ken");
  await expect(pairByPin(tv(), "0000", "Pixel")).rejects.toThrow("wrong PIN");
});

test("on the Setup hotspot a phone pairs with no PIN", async () => {
  const post: Post = async (path, body: any) =>
    path === "/api/pair/hotspot" && body.name === "Pixel"
      ? { status: 200, body: { token: "t0ken" } }
      : { status: 404, body: {} };
  expect(await pairOnHotspot(post, "Pixel")).toBe("t0ken");
});
