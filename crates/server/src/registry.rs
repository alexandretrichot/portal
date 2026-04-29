use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::mpsc;
use common::TunnelMessage;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct DeviceKey(pub String);

impl DeviceKey {
    pub fn new(key: &str) -> Self {
        Self(key.to_string())
    }
}

pub struct ConnectionHandle {
    pub session_id: String,
    pub device_id: String,
    pub gateway_id: String,
    pub device_alias: String,
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
            device_id: self.device_id.clone(),
            gateway_id: self.gateway_id.clone(),
            device_alias: self.device_alias.clone(),
            connected_at: self.connected_at,
            connected_at_utc: self.connected_at_utc,
            device_name: self.device_name.clone(),
            sender: self.sender.clone(),
            last_seen: self.last_seen.clone(),
        }
    }
}

pub struct DeviceInfo {
    pub device_id: String,
    pub alias: String,
    pub device_name: Option<String>,
    pub last_seen: DateTime<Utc>,
}

pub struct ConnectionRegistry {
    connections: DashMap<DeviceKey, ConnectionHandle>,
    gateway_devices: DashMap<String, HashSet<String>>,
    device_info: DashMap<String, DeviceInfo>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            gateway_devices: DashMap::new(),
            device_info: DashMap::new(),
        }
    }

    pub async fn register(
        &self,
        device_key: &str,
        device_id: &str,
        gateway_id: &str,
        device_alias: &str,
        handle: ConnectionHandle,
    ) {
        let key = DeviceKey::new(device_key);

        if let Some(old) = self.connections.remove(&key) {
            let disconnect_msg = TunnelMessage::Disconnect(common::DisconnectMessage {
                reason: common::DisconnectReason::TokenRevoked,
                message: Some("New connection established".into()),
            });
            let _ = old.1.sender.send(disconnect_msg).await;
        }

        self.device_info.insert(
            device_id.to_string(),
            DeviceInfo {
                device_id: device_id.to_string(),
                alias: device_alias.to_string(),
                device_name: handle.device_name.clone(),
                last_seen: Utc::now(),
            },
        );

        self.gateway_devices
            .entry(gateway_id.to_string())
            .or_default()
            .insert(device_key.to_string());

        self.connections.insert(key, handle);
    }

    pub fn get(&self, device_key: &str) -> Option<ConnectionHandle> {
        let key = DeviceKey::new(device_key);
        self.connections.get(&key).map(|r| r.clone())
    }

    pub fn get_device_info(&self, device_id: &str) -> Option<DeviceInfo> {
        self.device_info.get(device_id).map(|r| DeviceInfo {
            device_id: r.device_id.clone(),
            alias: r.alias.clone(),
            device_name: r.device_name.clone(),
            last_seen: r.last_seen,
        })
    }

    pub fn get_gateway_connections(&self, gateway_id: &str) -> Vec<ConnectionHandle> {
        let device_keys = match self.gateway_devices.get(gateway_id) {
            Some(keys) => keys.clone(),
            None => return vec![],
        };

        device_keys
            .iter()
            .filter_map(|key| self.get(key))
            .collect()
    }

    pub fn get_online_device_count(&self, gateway_id: &str) -> usize {
        match self.gateway_devices.get(gateway_id) {
            Some(keys) => keys
                .iter()
                .filter(|key| self.connections.contains_key(&DeviceKey::new(key)))
                .count(),
            None => 0,
        }
    }

    pub fn unregister(&self, device_key: &str) {
        let key = DeviceKey::new(device_key);
        if let Some((_, handle)) = self.connections.remove(&key) {
            if let Some(mut keys) = self.gateway_devices.get_mut(&handle.gateway_id) {
                keys.remove(device_key);
            }
            self.device_info.insert(
                handle.device_id.clone(),
                DeviceInfo {
                    device_id: handle.device_id,
                    alias: handle.device_alias,
                    device_name: handle.device_name,
                    last_seen: Utc::now(),
                },
            );
        }
    }

    pub fn is_online(&self, device_key: &str) -> bool {
        let key = DeviceKey::new(device_key);
        self.connections.contains_key(&key)
    }

    pub fn update_last_seen(&self, device_key: &str) {
        let key = DeviceKey::new(device_key);
        if let Some(handle) = self.connections.get(&key) {
            let now = Utc::now().timestamp() as u64;
            handle.last_seen.store(now, Ordering::Relaxed);

            if let Some(mut info) = self.device_info.get_mut(&handle.device_id) {
                info.last_seen = Utc::now();
            }
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
