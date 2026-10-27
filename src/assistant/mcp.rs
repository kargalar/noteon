use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::config;
use crate::error::Result;

use super::tools::{ToolHost, definitions};

const FALLBACK_PROTOCOL_VERSION: &str = "2025-06-18";

pub fn serve() -> Result<()> {
    let database = config::data_directory()?.join(config::DATABASE_FILE);
    let mut host = ToolHost::open(&database)?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(response) = handle_message(&mut host, &message) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_message(host: &mut ToolHost, message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => initialize_result(&params),
        "tools/list" => json!({ "tools": definitions() }),
        "tools/call" => call_tool(host, &params),
        "ping" => json!({}),
        _ => return Some(error_response(id, -32601, "Method not found")),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn initialize_result(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(FALLBACK_PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "noteon", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn call_tool(host: &mut ToolHost, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (text, is_error) = match host.call(name, &arguments) {
        Ok(value) => (value.to_string(), false),
        Err(error) => (error.to_string(), true),
    };
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_tools_and_reports_errors_as_tool_results() {
        let directory = tempfile::tempdir().unwrap();
        let mut host = ToolHost::open(&directory.path().join("t.sqlite")).unwrap();
        let list = handle_message(
            &mut host,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .unwrap();
        assert!(list["result"]["tools"].as_array().unwrap().len() > 15);
        let call = handle_message(
            &mut host,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "read_page", "arguments": {} } }),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], true);
        assert!(
            handle_message(
                &mut host,
                &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
            )
            .is_none()
        );
    }
}
