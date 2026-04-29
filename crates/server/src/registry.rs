use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use common::TunnelMessage;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct TokenHash(String);

impl TokenHash {
    pub fn from_token(token: &str) -> Self {
        let hash = Sha256::digest(token.as_bytes());
        Self(hex::encode(hash))
    }
}

pub struct ConnectionHandle {
    pub session_id: String,
    pub connected_at: Instant,
    pub connected_at_utc: DateTime<Utc>,
    pub device_name: Option<String>,
    pub sender: mpsc::Sender<TunnelMessage>,
    pub last_seen: Arc<AtomicU64>,
}

impl Clone for ConnectionHandle {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            connected_at: self.connected_at,
            connected_at_utc: self.connected_at_utc,
            device_name: self.device_name.clone(),
            sender: self.sender.clone(),
            last_seen: self.last_seen.clone(),
        }
    }
}

pub struct DeviceInfo {
    pub name: Option<String>,
    pub last_seen: DateTime<Utc>,
}

pub struct ConnectionRegistry {
    connections: DashMap<TokenHash, ConnectionHandle>,
    device_info: DashMap<TokenHash, DeviceInfo>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            device_info: DashMap::new(),
        }
    }

    pub async fn register(
        &self,
        token: &str,
        handle: ConnectionHandle,
    ) {
        let hash = TokenHash::from_token(token);

        if let Some(old) = self.connections.remove(&hash) {
            let disconnect_msg = TunnelMessage::Disconnect(common::DisconnectMessage {
                reason: common::DisconnectReason::TokenRevoked,
                message: Some("New connection established".into()),
            });
            let _ = old.1.sender.send(disconnect_msg).await;
        }

        self.device_info.insert(
            hash.clone(),
            DeviceInfo {
                name: handle.device_name.clone(),
                last_seen: Utc::now(),
            },
        );

        self.connections.insert(hash, handle);
    }

    pub fn get(&self, token: &str) -> Option<ConnectionHandle> {
        let hash = TokenHash::from_token(token);
        self.connections.get(&hash).map(|r| r.clone())
    }

    pub fn get_device_info(&self, token: &str) -> Option<DeviceInfo> {
        let hash = TokenHash::from_token(token);
        self.device_info.get(&hash).map(|r| DeviceInfo {
            name: r.name.clone(),
            last_seen: r.last_seen,
        })
    }

    pub fn unregister(&self, token: &str) {
        let hash = TokenHash::from_token(token);
        if let Some((_, handle)) = self.connections.remove(&hash) {
            self.device_info.insert(
                hash,
                DeviceInfo {
                    name: handle.device_name,
                    last_seen: Utc::now(),
                },
            );
        }
    }

    pub fn is_online(&self, token: &str) -> bool {
        let hash = TokenHash::from_token(token);
        self.connections.contains_key(&hash)
    }

    pub fn update_last_seen(&self, token: &str) {
        let hash = TokenHash::from_token(token);
        if let Some(handle) = self.connections.get(&hash) {
            let now = Utc::now().timestamp() as u64;
            handle.last_seen.store(now, Ordering::Relaxed);
        }
        if let Some(mut info) = self.device_info.get_mut(&hash) {
            info.last_seen = Utc::now();
        }
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
