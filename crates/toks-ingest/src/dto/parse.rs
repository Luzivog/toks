use crate::{scanner, ClientCounts, ClientId};

#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub client: String,
    pub model_id: String,
    pub provider_id: String,
    pub session_id: String,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
    pub timestamp: i64,
    pub date: String,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub duration_ms: Option<i64>,
    pub message_count: i32,
    pub agent: Option<String>,
}

pub struct ParsedMessages {
    pub messages: Vec<ParsedMessage>,
    pub counts: ClientCounts,
    pub processing_time_ms: u32,
}

impl Clone for ParsedMessages {
    fn clone(&self) -> Self {
        let mut counts = ClientCounts::new();
        for client in ClientId::iter() {
            counts.set(client, self.counts.get(client));
        }
        Self {
            messages: self.messages.clone(),
            counts,
            processing_time_ms: self.processing_time_ms,
        }
    }
}

impl std::fmt::Debug for ParsedMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("ParsedMessages");
        debug.field("messages", &self.messages);
        for client in ClientId::iter() {
            debug.field(client.as_str(), &self.counts.get(client));
        }
        debug.field("processing_time_ms", &self.processing_time_ms);
        debug.finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct LocalParseOptions {
    pub home_dir: Option<String>,
    pub use_env_roots: bool,
    pub clients: Option<Vec<String>>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub year: Option<String>,
    pub scanner_settings: scanner::ScannerSettings,
}
