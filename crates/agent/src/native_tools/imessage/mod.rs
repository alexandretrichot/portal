mod db;
mod tools;

pub use tools::{get_tools, handle_tool_call};

pub fn default_db_path() -> String {
    dirs::home_dir()
        .map(|h| h.join("Library/Messages/chat.db"))
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "~/Library/Messages/chat.db".to_string())
}
