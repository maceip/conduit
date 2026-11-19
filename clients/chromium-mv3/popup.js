/* The popup: the state of the tunnel in one line, the broker it talks
 * to, and a way to stop it.  Everything here is what the background
 * says; the popup keeps no state of its own. */
const statusLine = document.getElementById("status");
const toggle = document.getElementById("toggle");
const form = document.getElementById("configure");
const brokerInput = document.getElementById("broker");
const errorLine = document.getElementById("error");

const runtime = typeof browser !== "undefined" && browser.runtime ? browser : chrome;

function describe(reply) {
  if (reply.connected) {
    return "connected";
  }
  if (reply.pairing) {
    return "waiting for approval => approve in the broker tab";
  }
  if (!reply.paired) {
    return "not paired";
  }
  if (!reply.running) {
    return "stopped";
  }
  return "connecting";
}

function show(reply) {
  if (!reply || !reply.ok) {
    statusLine.textContent = "no answer from the tunnel";
    toggle.disabled = true;
    return;
  }
  statusLine.textContent = describe(reply);
  toggle.textContent = reply.running ? "stop" : "start";
  toggle.disabled = false;
  if (document.activeElement !== brokerInput && reply.broker) {
    brokerInput.value = reply.broker;
  }
  errorLine.textContent = reply.error ? String(reply.error) : "";
}

async function ask(message) {
  try {
    return await runtime.runtime.sendMessage(message);
  } catch (error) {
    return null;
  }
}

async function refresh() {
  show(await ask({ action: "status" }));
}

toggle.addEventListener("click", async () => {
  toggle.disabled = true;
  show(await ask({ action: "toggle" }));
});

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const reply = await ask({ action: "configure", broker: brokerInput.value });
  if (reply && !reply.ok && reply.error) {
    errorLine.textContent = reply.error;
    return;
  }
  brokerInput.blur();
  show(reply);
});

refresh();
setInterval(refresh, 500);
