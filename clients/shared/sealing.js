/*
 * The sealing, in the browser.
 *
 * The same scheme as the Rust: p256-aes256gcm/v1.
 *
 *   - the two ends agree a key with ECDH over P-256;
 *   - the shared secret is run through HKDF-SHA256 with the string of
 *     zeros RFC 5869 names, to make two directional keys;
 *   - the payload is sealed with AES-256-GCM under a nonce that carries
 *     the sequence number, with the sequence number also as the
 *     additional data;
 *   - the sealed blob is base64url without padding.
 *
 * Two choices were forced by what the browser's WebCrypto can do:
 * AES-GCM and not ChaCha20-Poly1305, which it does not have, and
 * P-256 and not X25519, whose raw private key it will not take with
 * the usage a derivation needs.
 *
 * The Rust side and this side are checked against the same vector, so a
 * change on one side that breaks the other fails the tests.
 */

const subtle = globalThis.crypto && globalThis.crypto.subtle;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/* The scheme this file implements. */
export const SCHEME = "p256-aes256gcm/v1";

/* The algorithm of the agreement. */
const CURVE = { name: "ECDH", namedCurve: "P-256" };

/* The salt of RFC 5869: the string of zeros. */
const SALT = new Uint8Array(32);

/* The two directions. */
export const TO_EXTENSION = "conduit:extension->client";
export const TO_CLIENT = "conduit:client->extension";

function requireWebCrypto() {
  if (!subtle) {
    throw new Error("this browser has no WebCrypto");
  }
  return subtle;
}

/* Base64url without padding. */
export function toBase64Url(bytes) {
  let text = "";
  for (const byte of new Uint8Array(bytes)) {
    text += String.fromCharCode(byte);
  }
  return btoa(text).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/* Base64url without padding, back to bytes. */
export function fromBase64Url(text) {
  const padded = text.replace(/-/g, "+").replace(/_/g, "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/* Import a private key from its scalar and its public point.
 *
 * WebCrypto takes an elliptic curve private key in the JWK form, which
 * carries the scalar and the two coordinates.  The public point is the
 * uncompressed point of SEC 1: the marker, the x coordinate and the y
 * coordinate, sixty-five bytes. */
export async function importPrivate(scalar, publicPoint) {
  const api = requireWebCrypto();
  const jwk = {
    kty: "EC",
    crv: "P-256",
    d: toBase64Url(scalar),
    x: toBase64Url(publicPoint.slice(1, 33)),
    y: toBase64Url(publicPoint.slice(33, 65)),
  };
  return api.importKey("jwk", jwk, CURVE, true, ["deriveBits"]);
}

/* A key pair from a secret key of 32 bytes and its public point. */
export async function keyPairFromSecret(scalar, publicPoint) {
  const privateKey = await importPrivate(scalar, publicPoint);
  return { privateKey, publicKey: null, publicBytes: publicPoint };
}

/* A new key pair. */
export async function generateKeyPair() {
  const api = requireWebCrypto();
  const pair = await api.generateKey(CURVE, true, ["deriveBits"]);
  return {
    privateKey: pair.privateKey,
    publicKey: pair.publicKey,
    publicBytes: new Uint8Array(await api.exportKey("raw", pair.publicKey)),
  };
}

/* The shared secret, and the two directional keys, from the other end's
 * public key. */
export async function agree(privateKey, peerPublicBytes) {
  const api = requireWebCrypto();
  const peer = await api.importKey("raw", peerPublicBytes, CURVE, false, []);
  const shared = new Uint8Array(
    await api.deriveBits({ name: "ECDH", public: peer }, privateKey, 256),
  );
  const base = await api.importKey("raw", shared, "HKDF", false, ["deriveBits"]);
  return {
    shared,
    toExtension: await deriveDirection(base, TO_EXTENSION),
    toClient: await deriveDirection(base, TO_CLIENT),
  };
}

/* One directional key. */
async function deriveDirection(baseKey, info) {
  const api = requireWebCrypto();
  const bits = await api.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: SALT, info: encoder.encode(info) },
    baseKey,
    256,
  );
  return new Uint8Array(bits);
}

/* The nonce: four random bytes and the sequence number. */
export function nonce(prefix, seq) {
  const nonce = new Uint8Array(12);
  nonce.set(prefix, 0);
  nonce.set(sequenceBytes(seq), 4);
  return nonce;
}

/* The sequence number, as eight bytes. */
export function sequenceBytes(seq) {
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  view.setBigUint64(0, BigInt(seq), false);
  return bytes;
}

/* Seal a payload. */
export async function seal(keyBytes, prefix, seq, plaintext) {
  const api = requireWebCrypto();
  const key = await api.importKey("raw", keyBytes, "AES-GCM", false, ["encrypt"]);
  const sealed = await api.encrypt(
    { name: "AES-GCM", iv: nonce(prefix, seq), additionalData: sequenceBytes(seq) },
    key,
    typeof plaintext === "string" ? encoder.encode(plaintext) : plaintext,
  );
  return new Uint8Array(sealed);
}

/* Open a sealed payload. */
export async function open(keyBytes, prefix, seq, sealed) {
  const api = requireWebCrypto();
  const key = await api.importKey("raw", keyBytes, "AES-GCM", false, ["decrypt"]);
  const opened = await api.decrypt(
    { name: "AES-GCM", iv: nonce(prefix, seq), additionalData: sequenceBytes(seq) },
    key,
    sealed,
  );
  return new Uint8Array(opened);
}

/* The text of a payload. */
export function toText(bytes) {
  return decoder.decode(bytes);
}
