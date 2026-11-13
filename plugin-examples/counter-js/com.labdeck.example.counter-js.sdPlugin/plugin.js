// Counter (JavaScript): each press adds one; the count is saved in the key's settings.
// Runs with Node.js 22 or newer, which has WebSocket built in (no packages needed).

// LabDeck starts the plugin with: -port <n> -pluginUUID <id> -registerEvent <name> -info <json>
const args = {};
for (let i = 2; i + 1 < process.argv.length; i += 2) {
  args[process.argv[i].replace(/^-/, "")] = process.argv[i + 1];
}

const ws = new WebSocket(`ws://127.0.0.1:${args.port}`);
const send = (msg) => ws.send(JSON.stringify(msg));

ws.onopen = () => send({ event: args.registerEvent, uuid: args.pluginUUID });

ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  const settings = msg.payload?.settings ?? {};
  if (msg.event === "willAppear") {
    send({ event: "setTitle", context: msg.context, payload: { title: String(settings.count ?? 0) } });
  } else if (msg.event === "keyDown") {
    const count = (settings.count ?? 0) + 1;
    send({ event: "setSettings", context: msg.context, payload: { count } });
    send({ event: "setTitle", context: msg.context, payload: { title: String(count) } });
    if (count % 10 === 0) send({ event: "showOk", context: msg.context });
  }
};

// LabDeck closes the connection when it quits; the plugin ends with it.
ws.onclose = () => process.exit(0);
