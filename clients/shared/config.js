/*
 * Where the broker is.
 *
 * The broker is a setting: the user types its tunnel address into the
 * popup and the background keeps it in storage. Everything else the
 * extension needs (the device-code endpoints, the approval page, the
 * tunnel url with its query) comes from that one address.
 */

export const DEFAULT_BROKER = "wss://broker.example/v1/tunnel";

/* The tunnel address, checked: ws:// or wss://, a host, no query.
 * null when it isn't one. */
export function parseBroker(text) {
  if (typeof text !== "string" || text.trim() === "") {
    return null;
  }
  let url;
  try {
    url = new URL(text.trim());
  } catch (error) {
    return null;
  }
  if (url.protocol !== "ws:" && url.protocol !== "wss:") {
    return null;
  }
  if (!url.hostname || url.search || url.hash || url.username || url.password) {
    return null;
  }
  if (url.pathname === "/" || url.pathname === "") {
    url.pathname = "/v1/tunnel";
  }
  return url.toString();
}

/* The http origin of the broker: ws => http, wss => https. */
export function httpOrigin(broker) {
  const url = new URL(broker);
  const protocol = url.protocol === "wss:" ? "https:" : "http:";
  return `${protocol}//${url.host}`;
}

/* The url of one of the broker's http endpoints, e.g. /v1/device/code,
 * or the approval page path the broker hands back. */
export function endpoint(broker, path) {
  return httpOrigin(broker) + path;
}

/* The url the extension opens its tunnel on. */
export function tunnelUrl(broker, surface, secret) {
  const url = new URL(broker);
  url.searchParams.set("role", "extension");
  url.searchParams.set("surface", surface);
  url.searchParams.set("secret", secret);
  return url.toString();
}
