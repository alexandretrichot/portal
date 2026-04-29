use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::mpsc;
use common::commands::{DiagnosticsResponse, McpServerStatus};
use common::{Message, TunnelMessage};

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
    pub transport_sender: mpsc::Sender<Message>,
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
            transport_sender: self.transport_sender.clone(),
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

#[derive(Debug, Clone)]
pub struct DeviceStatus {
    pub online: bool,
    pub diagnostics: Option<DiagnosticsResponse>,
    pub last_diagnostics_at: Option<DateTime<Utc>>,
    pub mcp_servers: HashMap<String, McpServerStatus>,
}

pub struct ConnectionRegistry {
    connections: DashMap<DeviceKey, ConnectionHandle>,
    gateway_devices: DashMap<String, HashSet<String>>,
    device_info: DashMap<String, DeviceInfo>,
    device_status: DashMap<String, DeviceStatus>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            gateway_devices: DashMap::new(),
            device_info: DashMap::new(),
            device_status: DashMap::new(),
        }
    }

    pub fn update_diagnostics(&self, device_id: &str, diagnostics: DiagnosticsResponse) {
        self.device_status
            .entry(device_id.to_string())
            .and_modify(|status| {
                status.diagnostics = Some(diagnostics.clone());
                status.last_diagnostics_at = Some(Utc::now());
            })
            .or_insert(DeviceStatus {
                online: self.is_device_online(device_id),
                diagnostics: Some(diagnostics),
                last_diagnostics_at: Some(Utc::now()),
                mcp_servers: HashMap::new(),
            });
    }

    pub fn update_mcp_status(&self, device_id: &str, servers: HashMap<String, McpServerStatus>) {
        self.device_status
            .entry(device_id.to_string())
            .and_modify(|status| {
                status.mcp_servers = servers.clone();
            })
            .or_insert(DeviceStatus {
                online: self.is_device_online(device_id),
                diagnostics: None,
                last_diagnostics_at: None,
                mcp_servers: servers,
            });
    }

    pub fn get_device_status(&self, device_id: &str) -> DeviceStatus {
        self.device_status
            .get(device_id)
            .map(|s| s.clone())
            .unwrap_or(DeviceStatus {
                online: self.is_device_online(device_id),
                diagnostics: None,
                last_diagnostics_at: None,
                mcp_servers: HashMap::new(),
            })
    }

    fn is_device_online(&self, device_id: &str) -> bool {
        self.connections.iter().any(|c| c.device_id == device_id)
    }

    pub async fn send_command(&self, device_key: &str, msg: Message) -> bool {
        if let Some(handle) = self.get(device_key) {
            handle.transport_sender.send(msg).await.is_ok()
        } else {
            false
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
