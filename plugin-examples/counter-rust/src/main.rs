//! Counter (Rust): each press adds one; the count is saved in the key's settings.
//!
//! LabDeck starts the program with `-port <n> -pluginUUID <id> -registerEvent <name> -info <json>`;
//! it connects to `ws://127.0.0.1:<n>`, registers, then answers LabDeck's events.

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
