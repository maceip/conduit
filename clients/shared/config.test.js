import { test } from "node:test";
import assert from "node:assert/strict";

import { DEFAULT_BROKER, endpoint, httpOrigin, parseBroker, tunnelUrl } from "./config.js";

test("a ws or wss address is a broker; anything else isn't", () => {
  assert.equal(parseBroker("ws://127.0.0.1:8787/v1/tunnel"), "ws://127.0.0.1:8787/v1/tunnel");
  assert.equal(parseBroker(" wss://broker.example/v1/tunnel "), "wss://broker.example/v1/tunnel");
  assert.equal(parseBroker(DEFAULT_BROKER), DEFAULT_BROKER);
  assert.equal(parseBroker("http://127.0.0.1:8787/v1/tunnel"), null);
  assert.equal(parseBroker("ws://127.0.0.1:8787/v1/tunnel?secret=x"), null);
  assert.equal(parseBroker("not a url"), null);
  assert.equal(parseBroker(""), null);
  assert.equal(parseBroker(undefined), null);
});

test("a bare host gets the tunnel path", () => {
  assert.equal(parseBroker("ws://127.0.0.1:8787"), "ws://127.0.0.1:8787/v1/tunnel");
});

test("the http endpoints sit at the broker's origin, not under the tunnel path", () => {
  assert.equal(httpOrigin("ws://127.0.0.1:8787/v1/tunnel"), "http://127.0.0.1:8787");
  assert.equal(httpOrigin("wss://broker.example/v1/tunnel"), "https://broker.example");
  assert.equal(
    endpoint("ws://127.0.0.1:8787/v1/tunnel", "/v1/device/code"),
    "http://127.0.0.1:8787/v1/device/code",
  );
});

test("the tunnel url carries the role, the surface and the secret", () => {
  const url = new URL(tunnelUrl("ws://127.0.0.1:8787/v1/tunnel", "dev.conduit.browser.tabs/v1", "s3cr+t"));
  assert.equal(url.origin + url.pathname, "ws://127.0.0.1:8787/v1/tunnel");
  assert.equal(url.searchParams.get("role"), "extension");
  assert.equal(url.searchParams.get("surface"), "dev.conduit.browser.tabs/v1");
  assert.equal(url.searchParams.get("secret"), "s3cr+t");
});
