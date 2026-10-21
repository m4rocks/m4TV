import { bytesToHex } from "@noble/hashes/utils.js";
import { proof } from "./proofs";

/** The wire to the TV: a JSON POST that returns the status and body. */
export type Post = (path: string, body: unknown) => Promise<{ status: number; body: any }>;

/** The QR link's fragment, `#secret=…&fp=…`, which a browser never sends to the server. */
export function parseLink(hash: string): { secret: string; fp: string } | null {
  const fields = new URLSearchParams(hash.replace(/^#/, ""));
  const secret = fields.get("secret");
  return secret ? { secret, fp: fields.get("fp") ?? "" } : null;
}

async function ok(post: Post, path: string, body: unknown): Promise<any> {
  const { status, body: reply } = await post(path, body);
  if (status !== 200) throw new Error(reply?.error ?? `the TV refused: ${status}`);
  return reply;
}

/** Pairs after a QR scan: proves the phone knows the secret, checks the TV does too, and works
 * out the token, which never travels. Mirrors crates/shell/src/remote/pairing.rs. */
export async function pairBySecret(post: Post, secret: string, name: string): Promise<string> {
  const { challenge } = await ok(post, "/api/pair/challenge", {});
  const nonce = bytesToHex(crypto.getRandomValues(new Uint8Array(16)));
  const reply = await ok(post, "/api/pair/prove", {
    challenge,
    nonce,
    proof: proof(secret, "phone", challenge, nonce),
    name,
  });
  if (reply.proof !== proof(secret, "tv", challenge, nonce)) {
    throw new Error("the TV did not prove it knows the secret");
  }
  return proof(secret, "token", challenge, nonce);
}

export async function pairByPin(post: Post, pin: string, name: string): Promise<string> {
  return (await ok(post, "/api/pair", { pin, name })).token;
}

/** Pairs on the Setup hotspot, where joining its WPA2 network with its password is the proof. */
export async function pairOnHotspot(post: Post, name: string): Promise<string> {
  return (await ok(post, "/api/pair/hotspot", { name })).token;
}
