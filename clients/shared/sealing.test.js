/*
 * The check that the browser and the Rust agree.
 *
 * The vector is the one the Rust test prints: the same two secret keys,
 * the same nonce prefix, the same sequence number, the same payload, and
 * therefore the same ciphertext.  If either side changes the way it
 * derives a key or builds a nonce, this fails.
 *
 * Run it with: node clients/shared/sealing.test.js
 */

import assert from "node:assert/strict";
import test from "node:test";
import {
  agree,
  fromBase64Url,
  keyPairFromSecret,
  open,
  seal,
  toBase64Url,
} from "./sealing.js";

const ONE_SECRET = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const TWO_SECRET = "2122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f40";

const ONE_PUBLIC = "04515c3d6eb9e396b904d3feca7f54fdcd0cc1e997bf375dca515ad0a6c3b4035f4536be3a50f318fbf9a5475902a221502bef0d57e08c53b2cc0a56f17d9f9354";
const TWO_PUBLIC = "041f140146bfb1b251f84f4ddbe0d4cdcfd77afd984a9520e35794021f8312bb9eec995a08b1fa7704df3dcc0b50a9665263fb7711f95f9f8a449c5096e47c892b";
const SEALED = "q9rhXAyOIxxxfqaOxOdhv6nEUmkWAS8";

const PREFIX = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);

function hex(bytes) {
  return [...new Uint8Array(bytes)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

test("the public key of a secret key is the one the Rust names", async () => {
  const one = await keyPairFromSecret(hexBytes(ONE_SECRET), hexBytes(ONE_PUBLIC));
  assert.equal(hex(one.publicBytes), ONE_PUBLIC);
  const two = await keyPairFromSecret(hexBytes(TWO_SECRET), hexBytes(TWO_PUBLIC));
  assert.equal(hex(two.publicBytes), TWO_PUBLIC);
});

test("the sealed payload is the one the Rust seals", async () => {
  const one = await keyPairFromSecret(hexBytes(ONE_SECRET), hexBytes(ONE_PUBLIC));
  const two = await keyPairFromSecret(hexBytes(TWO_SECRET), hexBytes(TWO_PUBLIC));

  const shared = await agree(one.privateKey, two.publicBytes);
  const sealed = await seal(shared.toExtension, PREFIX, 1, "conduit");
  assert.equal(toBase64Url(sealed), SEALED);
});

test("a sealed payload opens again", async () => {
  const one = await keyPairFromSecret(hexBytes(ONE_SECRET), hexBytes(ONE_PUBLIC));
  const two = await keyPairFromSecret(hexBytes(TWO_SECRET), hexBytes(TWO_PUBLIC));

  const shared = await agree(one.privateKey, two.publicBytes);
  const sealed = await seal(shared.toExtension, PREFIX, 1, "the message");
  const opened = await open(shared.toExtension, PREFIX, 1, sealed);
  assert.equal(new TextDecoder().decode(opened), "the message");
});

test("a payload does not open under another sequence number", async () => {
  const one = await keyPairFromSecret(hexBytes(ONE_SECRET), hexBytes(ONE_PUBLIC));
  const two = await keyPairFromSecret(hexBytes(TWO_SECRET), hexBytes(TWO_PUBLIC));

  const shared = await agree(one.privateKey, two.publicBytes);
  const sealed = await seal(shared.toExtension, PREFIX, 1, "the message");
  await assert.rejects(() => open(shared.toExtension, PREFIX, 2, sealed));
});

test("the two directions do not share a key", async () => {
  const one = await keyPairFromSecret(hexBytes(ONE_SECRET), hexBytes(ONE_PUBLIC));
  const two = await keyPairFromSecret(hexBytes(TWO_SECRET), hexBytes(TWO_PUBLIC));

  const shared = await agree(one.privateKey, two.publicBytes);
  assert.notEqual(hex(shared.toExtension), hex(shared.toClient));
});

test("base64url survives the round trip", async () => {
  const bytes = new Uint8Array([251, 255, 190, 0, 16, 240]);
  const text = toBase64Url(bytes);
  assert.ok(!text.includes("="));
  assert.ok(!text.includes("+"));
  assert.ok(!text.includes("/"));
  assert.equal(hex(fromBase64Url(text)), hex(bytes));
});

function hexBytes(text) {
  const bytes = new Uint8Array(text.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = parseInt(text.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}
