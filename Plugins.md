# Writing LabDeck plugins

A plugin adds actions to LabDeck's action list. You put the action on a key like any other. When
the key is pressed, LabDeck tells the plugin, and the plugin can change the key's title, picture
and settings. You can write plugins in **Rust** (or any language that makes a program),
**JavaScript**, **TypeScript** and **Lua**.

- Rust, JavaScript and TypeScript plugins are separate programs. LabDeck starts them and they talk
  to LabDeck over a WebSocket on this computer (`ws://127.0.0.1:<port>`), sending JSON messages.
- Lua plugins run inside LabDeck. There's nothing to install, and no WebSocket: LabDeck calls your
  script's functions, and your script calls LabDeck's.

The messages are the same in every language, and they follow the standard Stream Deck plugin
protocol, so plugins written for it that don't need a settings web page can run unchanged.

Contents:

1. [A plugin's folder](#a-plugins-folder)
2. [manifest.json](#manifestjson)
3. [How LabDeck runs a plugin](#how-labdeck-runs-a-plugin)
4. [Messages from LabDeck](#messages-from-labdeck)
5. [Messages to LabDeck](#messages-to-labdeck)
6. [Rust](#rust) · [JavaScript](#javascript) · [TypeScript](#typescript) · [Lua](#lua)
7. [Settings in the key editor](#settings-in-the-key-editor)
8. [Testing and troubleshooting](#testing-and-troubleshooting)
9. [What isn't supported yet](#what-isnt-supported-yet)

Every example here is a working **Counter** plugin in the `plugin-examples` folder: each press adds
one to the key's title, the count is saved on the key, and every 10th press flashes the key green.

---

## A plugin's folder

A plugin is one folder whose name ends in `.sdPlugin`, usually named after the plugin's id:

```text
com.example.counter.sdPlugin/
├── manifest.json      what the plugin is and which actions it has
└── main.lua           the code (or plugin.js, plugin.ts, a program…)
```

Put the folder in LabDeck's **Plugins** folder, then restart LabDeck. It starts plugins when it
starts.

| System | Plugins folder |
|---|---|
| Windows | `%APPDATA%\LabDeck\Plugins` |
| Linux and macOS | `~/.local/share/LabDeck/Plugins` (or `$XDG_DATA_HOME/LabDeck/Plugins`) |

If the environment variable `LABDECK_HOME` is set, the Plugins folder is inside it instead. Create
the folder if it isn't there yet.

To try an example, copy one of these folders into the Plugins folder:

- `plugin-examples/counter-lua/com.labdeck.example.counter-lua.sdPlugin`
- `plugin-examples/counter-js/com.labdeck.example.counter-js.sdPlugin` (needs Node.js 22 or newer)
- `plugin-examples/counter-ts/com.labdeck.example.counter-ts.sdPlugin` (needs Node.js 23.6 or newer)
- `plugin-examples/counter-rust/com.labdeck.example.counter-rust.sdPlugin`, after building the
  program into it (see [Rust](#rust))

---

## manifest.json

```json
{
  "UUID": "com.example.counter",
  "Name": "Counter",
  "Version": "1.0.0",
  "Author": "You",
  "Description": "Each press adds one.",
  "CodePath": "main.lua",
  "Category": "Counter",
  "Actions": [
    {
      "UUID": "com.example.counter.count",
      "Name": "Counter",
      "Tooltip": "Each press adds one",
      "States": [{ "Title": "0" }]
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `UUID` | The plugin's id, in reverse-domain form. It's also the `-pluginUUID` LabDeck passes. Without one, the folder name (minus `.sdPlugin`) is used. |
| `Name` | Shown in messages about the plugin. |
| `Version` | Passed to the plugin in `-info`. |
| `CodePath` | The file to run, relative to the folder. |
| `CodePathWin`, `CodePathMac`, `CodePathLin` | Used instead of `CodePath` on that system. Handy for programs: `counter.exe` on Windows, `counter` elsewhere. |
| `Category` | The group the actions are listed under in the action list (else `Name`). |
| `Actions` | One entry per action. |

Each action:

| Field | Meaning |
|---|---|
| `UUID` | The action's id. Start it with the plugin's id: `com.example.counter.count`. |
| `Name`, `Tooltip` | Shown in the action list. |
| `States` | One entry per state. With two or more, the key moves to the next state after each press, unless `DisableAutomaticStates` is `true`. |
| `DisableAutomaticStates` | `true`: only the plugin changes the state (with `setState`). |
| `Controllers` | `["Keypad"]` (the default). Actions only for dials are left out of the list. |
| `VisibleInActionsList` | `false` hides the action from the list. |
| `SupportedInMultiActions` | `false`: the action can't be a Multi Action step. |

---

## How LabDeck runs a plugin

LabDeck looks at the end of `CodePath`:

| `CodePath` ends in | LabDeck runs |
|---|---|
| `.lua` | the script, inside LabDeck (Lua 5.4) |
| `.js`, `.mjs`, `.cjs` | `node <file>` (Node.js; set `LABDECK_NODE` to use another `node`) |
| `.ts`, `.mts`, `.cts` | `node <file>` (Node.js 23.6 or newer runs TypeScript directly) |
| anything else | the file itself, as a program. On Windows `.exe` is added if the file isn't found without it. |

A program or Node plugin is started in its own folder with these arguments:

```text
-port 51234 -pluginUUID com.example.counter -registerEvent registerPlugin -info {"application":{…},"plugin":{…},…}
```

| Argument | Meaning |
|---|---|
| `-port` | Connect to `ws://127.0.0.1:<port>`. |
| `-pluginUUID` | Your plugin's id. Send it back when you register. |
| `-registerEvent` | The event name to register with (`registerPlugin`). |
| `-info` | JSON: `application` (`platform` `windows`, `mac` or `linux`, `version`, `language`), `plugin` (`uuid`, `version`), `devicePixelRatio`. Devices arrive as `deviceDidConnect` messages. |

The first message a plugin sends registers it:

```json
{ "event": "registerPlugin", "uuid": "com.example.counter" }
```

After that, messages go both ways. When LabDeck quits it closes the connection and ends the
program, so your plugin should exit when the connection closes. Anything a program prints goes to
its log file (see [Testing](#testing-and-troubleshooting)), and on Windows no console window opens.

Each key with one of your actions has a **context**: an id for that key, the same every time. Use it
to say which key a command is for.

---

## Messages from LabDeck

All are JSON objects with an `event` field.

### willAppear / willDisappear

The key is now shown on the deck (its page or folder opened, or the key was just placed), or no
longer shown. Set its title or picture in `willAppear`.

```json
{
  "event": "willAppear",
  "action": "com.example.counter.count",
  "context": "8B5C2D1E-…",
  "device": "AL12K2C01234",
  "payload": {
    "settings": { "count": 4 },
    "coordinates": { "column": 2, "row": 1 },
    "state": 0,
    "isInMultiAction": false,
    "controller": "Keypad"
  }
}
```

### keyDown / keyUp

The key was pressed or let go. Same fields as `willAppear`. In a Multi Action the step gets
`keyDown` then `keyUp` straight away, with `isInMultiAction: true`. Pressing **Run** in the key
editor sends both too.

### didReceiveSettings

The key's settings changed in the key editor, or you asked with `getSettings`. Same fields as
`willAppear`.

### didReceiveGlobalSettings

The answer to `getGlobalSettings`:

```json
{ "event": "didReceiveGlobalSettings", "payload": { "settings": { "apiKey": "…" } } }
```

### deviceDidConnect / deviceDidDisconnect

```json
{
  "event": "deviceDidConnect",
  "device": "AL12K2C01234",
  "deviceInfo": { "name": "Stream Deck XL", "type": 2, "size": { "columns": 8, "rows": 4 } }
}
```

`type`: 0 Stream Deck, 1 Mini, 2 XL, 5 Pedal, 7 +, 9 Neo. It's sent when your plugin registers (if a
device is connected) and whenever a device connects. `deviceDidDisconnect` has `event` and `device`.

---

## Messages to LabDeck

Send a JSON object with `event`, the key's `context` where there is one, and a `payload`.

| Event | Payload | What it does |
|---|---|---|
| `setTitle` | `{ "title": "42", "state": 0 }` | Shows that title on the key. Without `title`, the key's own title comes back. Without `state`, it's for every state. |
| `setImage` | `{ "image": "data:image/png;base64,…", "state": 0 }` | Shows that picture: a base64 `data:` address of a PNG, JPEG, GIF, BMP or WebP, or a file path (absolute, or inside your plugin's folder). Without `image`, the key's own picture comes back. |
| `setState` | `{ "state": 1 }` | Moves the key to that state (saved with the profile). |
| `setSettings` | any JSON object | Replaces the key's settings (saved with the profile). They come back in every event's `payload.settings`. |
| `getSettings` | (none) | LabDeck answers with `didReceiveSettings`. |
| `setGlobalSettings` | any JSON object | Saves settings for the whole plugin (put your plugin's id in `context`). |
| `getGlobalSettings` | (none) | LabDeck answers with `didReceiveGlobalSettings`. |
| `showOk` | (none) | The key flashes green for 1.5 s. |
| `showAlert` | (none) | The key flashes red for 1.5 s. |
| `openUrl` | `{ "url": "https://…" }` | Opens the address in the default browser. |
| `logMessage` | `{ "message": "…" }` | Writes a line to your plugin's log file. |
| `switchToProfile` | `{ "profile": "Gaming", "page": 0 }` | Switches to the profile with that name (and page, counted from 0). |

Titles and pictures set with `setTitle` and `setImage` last while LabDeck runs; settings and states
are saved.

```json
{ "event": "setTitle", "context": "8B5C2D1E-…", "payload": { "title": "5" } }
```

---

## Rust

The example is `plugin-examples/counter-rust`: a small program using the `tungstenite` (WebSocket)
and `serde_json` crates.

```toml
[dependencies]
serde_json = "1"
tungstenite = { version = "0.26", default-features = false, features = ["handshake"] }
```

```rust
use serde_json::{json, Value};
use std::net::TcpStream;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

type Socket = WebSocket<MaybeTlsStream<TcpStream>>;

fn send(ws: &mut Socket, msg: Value) -> bool {
    ws.send(Message::text(msg.to_string())).is_ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned().unwrap_or_default();

    let (mut ws, _) = connect(format!("ws://127.0.0.1:{}", arg("-port"))).expect("LabDeck's plugin port");
    send(&mut ws, json!({"event": arg("-registerEvent"), "uuid": arg("-pluginUUID")}));

    loop {
        let text = match ws.read() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => break, // LabDeck quit
            Ok(_) => continue,
        };
        let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
        let context = msg["context"].clone();
        let count = msg["payload"]["settings"]["count"].as_u64().unwrap_or(0);
        match msg["event"].as_str() {
            Some("willAppear") => {
                send(&mut ws, json!({"event": "setTitle", "context": context, "payload": {"title": count.to_string()}}));
            }
            Some("keyDown") => {
                let count = count + 1;
                send(&mut ws, json!({"event": "setSettings", "context": context, "payload": {"count": count}}));
                send(&mut ws, json!({"event": "setTitle", "context": context, "payload": {"title": count.to_string()}}));
                if count % 10 == 0 {
                    send(&mut ws, json!({"event": "showOk", "context": context}));
                }
            }
            _ => {}
        }
    }
}
```

Build it and put the program in the plugin's folder:

```sh
cd plugin-examples/counter-rust
cargo build --release
# Windows:
copy target\release\counter-rust.exe com.labdeck.example.counter-rust.sdPlugin\
# Linux / macOS:
cp target/release/counter-rust com.labdeck.example.counter-rust.sdPlugin/
```

The manifest names the program per system:

```json
"CodePathWin": "counter-rust.exe",
"CodePathMac": "counter-rust",
"CodePathLin": "counter-rust",
```

A program built on one system only runs on that system, so build it on each system you want the
plugin on. The same protocol works from any language that can open a WebSocket: start from the
arguments, register, then read and send JSON.

---

## JavaScript

Needs **Node.js 22 or newer** (it has WebSocket built in, so no packages are needed). The example
is `plugin-examples/counter-js`; `CodePath` is `plugin.js`.

```js
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

ws.onclose = () => process.exit(0);
```

Packages from npm work as usual: run `npm install` in the plugin's folder, so `node_modules` sits
next to your code. LabDeck finds `node` on the PATH; if it's somewhere else, set the environment
variable `LABDECK_NODE` to its full path before starting LabDeck.

---

## TypeScript

The example is `plugin-examples/counter-ts`. There are two ways to run it:

- **Node.js 23.6 or newer** runs `.ts` files as they are (it removes the types), so `CodePath` can
  be `plugin.ts`. Use only types that can simply be removed (interfaces, type annotations); not
  `enum` or `namespace`.
- **Older Node.js**: compile to JavaScript and point `CodePath` at the result:

  ```sh
  cd plugin-examples/counter-ts
  npm install --save-dev typescript @types/node
  npx tsc          # writes com.labdeck.example.counter-ts.sdPlugin/plugin.js
  ```

  then set `"CodePath": "plugin.js"` in the manifest.

```ts
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
```

---

## Lua

Lua plugins run inside LabDeck (Lua 5.4, with its standard libraries), so they need nothing
installed. `CodePath` names the script. The example is `plugin-examples/counter-lua`.

For every message from LabDeck, LabDeck calls the **global function with the event's name**, if
your script has one, with the message as a table (`ev.context`, `ev.payload.settings`, …). If the
script has a `tick` function, it's called every second with the seconds since the plugin started.

```lua
function willAppear(ev)
  local count = ev.payload.settings.count or 0
  labdeck.setTitle(ev.context, tostring(count))
end

function keyDown(ev)
  local count = (ev.payload.settings.count or 0) + 1
  labdeck.setSettings(ev.context, { count = count })
  labdeck.setTitle(ev.context, tostring(count))
  if count % 10 == 0 then
    labdeck.showOk(ev.context)
  end
end
```

The `labdeck` table:

| Name | What it does |
|---|---|
| `labdeck.uuid` | Your plugin's id. |
| `labdeck.folder` | Your plugin's folder (for reading its own files). |
| `labdeck.setTitle(context, title [, state])` | `setTitle` |
| `labdeck.setImage(context, image [, state])` | `setImage`: a `data:` address or a file path |
| `labdeck.setState(context, state)` | `setState` |
| `labdeck.setSettings(context, table)` | `setSettings` |
| `labdeck.getSettings(context)` | `getSettings` (answered by your `didReceiveSettings`) |
| `labdeck.setGlobalSettings(table)` | `setGlobalSettings` |
| `labdeck.getGlobalSettings()` | `getGlobalSettings` (answered by your `didReceiveGlobalSettings`) |
| `labdeck.showOk(context)`, `labdeck.showAlert(context)` | flash the key green or red |
| `labdeck.openUrl(url)` | `openUrl` |
| `labdeck.log(message)` | `logMessage` |
| `labdeck.send(table)` | send any message from [Messages to LabDeck](#messages-to-labdeck) as a table |

A Lua error in one of your functions is written to the plugin's log file, and the plugin keeps
running. An error while the script first loads stops the plugin; the reason shows when you hover
over its actions in the action list. Your functions run
one at a time on the plugin's own thread, so a slow function delays your plugin's next events, but
not LabDeck.

---

## Settings in the key editor

Select a key with your action, and the key editor shows the plugin's name, whether it's running,
and the key's settings as JSON. Edit them there: once the text is a valid JSON object it's saved,
and your plugin gets `didReceiveSettings`. Read your options from `payload.settings`, and keep them
in a JSON object your users can edit by hand.

---

## Testing and troubleshooting

- **Log files**: everything a program plugin prints, and every `logMessage` (and Lua error), goes
  to `Logs/<plugin id>.log` in LabDeck's folder (`%APPDATA%\LabDeck\Logs` on Windows,
  `~/.local/share/LabDeck/Logs` on Linux and macOS).
- **The plugin's actions are grey in the action list**: hover over one to see why it isn't
  running, or select one of its keys and read the key editor. Typical reasons: Node.js isn't
  installed, `CodePath` names a file that isn't there, or the program stopped.
- **Changes don't show up**: LabDeck reads plugins when it starts. Quit LabDeck from its tray icon
  and start it again.
- **The plugin isn't in the list at all**: check that the folder name ends in `.sdPlugin`, it's
  directly inside the Plugins folder, and `manifest.json` is valid JSON with an `Actions` list.
- **Test without a device**: put the key in the window, select it and press **Run** in the key
  editor; your plugin gets `keyDown` and `keyUp`.

---

## What isn't supported yet

- Settings web pages (`PropertyInspectorPath`) aren't shown; use the JSON settings in the key
  editor. `sendToPlugin` and `sendToPropertyInspector` have no page to talk to.
- Dials and touch screens (`dialRotate`, `setFeedback`, …), `titleParametersDidChange`,
  `applicationDidLaunch` / `applicationDidTerminate` and `systemDidWakeUp` aren't sent.
- SVG pictures in `setImage`; send a PNG instead.
- Plugin icons from the manifest: actions are listed with LabDeck's plugin icon.
- Plugins that crash aren't restarted until LabDeck starts again.
