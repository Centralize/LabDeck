// Counter (TypeScript): each press adds one; the count is saved in the key's settings.
// Node.js 23.6 or newer runs this file as it is (it strips the types). With an older Node,
// compile it first (`npx tsc`) and point CodePath at the plugin.js it makes.

/** A message from LabDeck (only the fields this plugin reads). */
interface LabDeckEvent {
  event: string;
  context?: string;
  payload?: { settings?: { count?: number } };
}

/** A message to LabDeck. */
interface Command {
  event: string;
  uuid?: string;
  context?: string;
  payload?: unknown;
}

// LabDeck starts the plugin with: -port <n> -pluginUUID <id> -registerEvent <name> -info <json>
const args: Record<string, string> = {};
for (let i = 2; i + 1 < process.argv.length; i += 2) {
  args[process.argv[i].replace(/^-/, "")] = process.argv[i + 1];
}

const ws = new WebSocket(`ws://127.0.0.1:${args.port}`);
const send = (msg: Command): void => ws.send(JSON.stringify(msg));

ws.onopen = () => send({ event: args.registerEvent, uuid: args.pluginUUID });

ws.onmessage = (e: MessageEvent) => {
  const msg = JSON.parse(String(e.data)) as LabDeckEvent;
  const count = msg.payload?.settings?.count ?? 0;
  if (msg.event === "willAppear") {
    send({ event: "setTitle", context: msg.context, payload: { title: String(count) } });
  } else if (msg.event === "keyDown") {
    const next = count + 1;
    send({ event: "setSettings", context: msg.context, payload: { count: next } });
    send({ event: "setTitle", context: msg.context, payload: { title: String(next) } });
    if (next % 10 === 0) send({ event: "showOk", context: msg.context });
  }
};

ws.onclose = () => process.exit(0);
