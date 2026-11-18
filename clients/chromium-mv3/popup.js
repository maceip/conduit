/* The popup: the surface of the tunnel, in one line. */
const status = document.getElementById("status");
const toggle = document.getElementById("toggle");

const runtime = typeof browser !== "undefined" ? browser : chrome;

async function refresh() {
  const reply = await runtime.runtime.sendMessage({ action: "status" });
  if (reply && reply.ok) {
    status.textContent = reply.connected ? "connected" : "not connected";
    toggle.textContent = reply.connected ? "stop" : "start";
  } else {
    status.textContent = "the tunnel did not answer";
  }
}

toggle.addEventListener("click", async () => {
  await runtime.runtime.sendMessage({ action: "toggle" });
  await refresh();
});

refresh();
