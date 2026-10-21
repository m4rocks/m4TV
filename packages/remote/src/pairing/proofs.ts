import { hmac } from "@noble/hashes/hmac.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";

export type Role = "phone" | "tv" | "token";

/** HMAC-SHA256 of `role:challenge:nonce` under the QR secret (hex), as hex. The Remote server
 * computes the same (crates/shell/src/remote/pairing.rs): the phone sends the "phone" proof,
 * checks the TV's "tv" proof, and works out its token, which never travels. */
export function proof(secret: string, role: Role, challenge: string, nonce: string): string {
  const message = new TextEncoder().encode(`${role}:${challenge}:${nonce}`);
  return bytesToHex(hmac(sha256, hexToBytes(secret), message));
}
