use dashmap::DashMap;
use tokio::sync::oneshot;
use common::McpResponseMessage;
use uuid::Uuid;

pub struct PendingRequests {
    requests: DashMap<Uuid, oneshot::Sender<McpResponseMessage>>,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            requests: DashMap::new(),
        }
    }

    pub fn register(&self, correlation_id: Uuid) -> oneshot::Receiver<McpResponseMessage> {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(correlation_id, tx);
        rx
    }

    pub fn complete(&self, response: McpResponseMessage) -> bool {
        if let Some((_, tx)) = self.requests.remove(&response.correlation_id) {
            tx.send(response).is_ok()
        } else {
            false
        }
    }

    pub fn cancel(&self, correlation_id: &Uuid) {
        self.requests.remove(correlation_id);
    }

    pub fn pending_count(&self) -> usize {
        self.requests.len()
    }
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}
