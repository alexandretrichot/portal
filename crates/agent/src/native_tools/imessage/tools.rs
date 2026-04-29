use std::process::Command;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorData, Tool, Content, JsonObject};
use serde_json::json;

use super::db;

fn make_tool(name: &'static str, description: &'static str, schema: serde_json::Value) -> Tool {
    let schema_obj: JsonObject = schema.as_object().cloned().unwrap_or_default();
    let mut tool = Tool::default();
    tool.name = std::borrow::Cow::Borrowed(name);
    tool.description = Some(std::borrow::Cow::Borrowed(description));
    tool.input_schema = Arc::new(schema_obj);
    tool
}

pub fn get_tools() -> Vec<Tool> {
    vec![
        make_tool(
            "list_conversations",
            "List recent iMessage conversations. Returns chat IDs, participants, and last message date.",
            json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of conversations to return (default: 20)"
                    }
                }
            }),
        ),
        make_tool(
            "get_messages",
            "Get messages from a specific conversation by chat_id.",
            json!({
                "type": "object",
                "properties": {
                    "chat_id": {
                        "type": "string",
                        "description": "The chat identifier (e.g., '+1234567890' or 'chat123456')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of messages to return (default: 50)"
                    }
                },
                "required": ["chat_id"]
            }),
        ),
        make_tool(
            "search_messages",
            "Search for messages containing specific text across all conversations.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text to search for in messages"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 20)"
                    }
                },
                "required": ["query"]
            }),
        ),
        make_tool(
            "send_message",
            "Send an iMessage to a phone number or email address.",
            json!({
                "type": "object",
                "properties": {
                    "recipient": {
                        "type": "string",
                        "description": "Phone number or email address to send to"
                    },
                    "message": {
                        "type": "string",
                        "description": "The message text to send"
                    }
                },
                "required": ["recipient", "message"]
            }),
        ),
    ]
}

pub async fn handle_tool_call(db_path: &str, request: CallToolRequestParams) -> Result<CallToolResult, ErrorData> {
    let args = request.arguments.unwrap_or_default();

    match request.name.as_ref() {
        "list_conversations" => {
            let limit = args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;

            match db::list_conversations(db_path, limit) {
                Ok(conversations) => Ok(CallToolResult::success(vec![
                    Content::text(serde_json::to_string_pretty(&conversations).unwrap_or_default())
                ])),
                Err(e) => Ok(CallToolResult::error(vec![
                    Content::text(format!("Error listing conversations: {}", e))
                ])),
            }
        }

        "get_messages" => {
            let chat_id = args.get("chat_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ErrorData::invalid_params("chat_id is required", None))?;

            let limit = args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;

            match db::get_messages(db_path, chat_id, limit) {
                Ok(messages) => Ok(CallToolResult::success(vec![
                    Content::text(serde_json::to_string_pretty(&messages).unwrap_or_default())
                ])),
                Err(e) => Ok(CallToolResult::error(vec![
                    Content::text(format!("Error getting messages: {}", e))
                ])),
            }
        }

        "search_messages" => {
            let query = args.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ErrorData::invalid_params("query is required", None))?;

            let limit = args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;

            match db::search_messages(db_path, query, limit) {
                Ok(messages) => Ok(CallToolResult::success(vec![
                    Content::text(serde_json::to_string_pretty(&messages).unwrap_or_default())
                ])),
                Err(e) => Ok(CallToolResult::error(vec![
                    Content::text(format!("Error searching messages: {}", e))
                ])),
            }
        }

        "send_message" => {
            let recipient = args.get("recipient")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ErrorData::invalid_params("recipient is required", None))?;

            let message = args.get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ErrorData::invalid_params("message is required", None))?;

            match send_imessage(recipient, message) {
                Ok(_) => Ok(CallToolResult::success(vec![
                    Content::text(json!({
                        "success": true,
                        "recipient": recipient,
                        "message": message
                    }).to_string())
                ])),
                Err(e) => Ok(CallToolResult::error(vec![
                    Content::text(format!("Error sending message: {}", e))
                ])),
            }
        }

        _ => Err(ErrorData::invalid_params("Unknown tool", None)),
    }
}

fn send_imessage(recipient: &str, message: &str) -> anyhow::Result<()> {
    let script = format!(
        r#"
        tell application "Messages"
            set targetService to 1st account whose service type = iMessage
            set targetBuddy to participant "{}" of targetService
            send "{}" to targetBuddy
        end tell
        "#,
        recipient.replace("\"", "\\\""),
        message.replace("\"", "\\\"").replace("\n", "\\n")
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("AppleScript error: {}", stderr);
    }

    Ok(())
}
