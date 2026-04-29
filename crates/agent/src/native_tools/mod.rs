#[cfg(target_os = "macos")]
mod imessage;

use rmcp::model::{CallToolRequestParams, CallToolResult, ErrorData, Tool};

pub struct NativeTools {
    #[cfg(target_os = "macos")]
    imessage_db_path: String,
}

impl NativeTools {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            imessage_db_path: imessage::default_db_path(),
        }
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();

        #[cfg(target_os = "macos")]
        {
            tools.extend(imessage::get_tools());
        }

        tools
    }

    pub async fn handle_tool_call(
        &self,
        request: CallToolRequestParams,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.name.as_ref();

        #[cfg(target_os = "macos")]
        {
            let imessage_tools = ["list_conversations", "get_messages", "search_messages", "send_message"];
            if imessage_tools.contains(&tool_name) {
                return imessage::handle_tool_call(&self.imessage_db_path, request).await;
            }
        }

        Err(ErrorData::invalid_params(
            format!("Unknown native tool: {}", tool_name),
            None,
        ))
    }
}

impl Default for NativeTools {
    fn default() -> Self {
        Self::new()
    }
}
