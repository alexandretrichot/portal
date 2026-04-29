use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::sync::{Arc, Mutex};
use tokio::task;
use uuid::Uuid;

use super::models::{Device, DeviceWithGateway, Gateway, McpConfig, User};

#[derive(Clone)]
pub struct Repository {
    conn: Arc<Mutex<Connection>>,
}

impl Repository {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    fn generate_key() -> String {
        let bytes: [u8; 16] = rand::random();
        hex::encode(bytes)
    }

    fn parse_datetime(s: &str) -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    pub async fn get_or_create_user(&self, clerk_user_id: &str, email: &str) -> Result<User> {
        let conn = self.conn.clone();
        let clerk_user_id = clerk_user_id.to_string();
        let email = email.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();

            let existing: Option<User> = conn
                .query_row(
                    "SELECT id, clerk_user_id, email, created_at FROM users WHERE clerk_user_id = ?",
                    params![&clerk_user_id],
                    |row| {
                        Ok(User {
                            id: row.get(0)?,
                            clerk_user_id: row.get(1)?,
                            email: row.get(2)?,
                            created_at: Self::parse_datetime(&row.get::<_, String>(3)?),
                        })
                    },
                )
                .ok();

            if let Some(user) = existing {
                return Ok(user);
            }

            let id = Uuid::new_v4().to_string();
            let now = Utc::now();

            conn.execute(
                "INSERT INTO users (id, clerk_user_id, email, created_at) VALUES (?, ?, ?, ?)",
                params![&id, &clerk_user_id, &email, now.to_rfc3339()],
            )?;

            let gateway_id = Uuid::new_v4().to_string();
            let gateway_key = Self::generate_key();

            conn.execute(
                "INSERT INTO gateways (id, user_id, key, created_at) VALUES (?, ?, ?, ?)",
                params![&gateway_id, &id, &gateway_key, now.to_rfc3339()],
            )?;

            Ok(User {
                id,
                clerk_user_id,
                email,
                created_at: now,
            })
        })
        .await?
    }

    pub async fn get_user_by_clerk_id(&self, clerk_user_id: &str) -> Result<Option<User>> {
        let conn = self.conn.clone();
        let clerk_user_id = clerk_user_id.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let user = conn
                .query_row(
                    "SELECT id, clerk_user_id, email, created_at FROM users WHERE clerk_user_id = ?",
                    params![&clerk_user_id],
                    |row| {
                        Ok(User {
                            id: row.get(0)?,
                            clerk_user_id: row.get(1)?,
                            email: row.get(2)?,
                            created_at: Self::parse_datetime(&row.get::<_, String>(3)?),
                        })
                    },
                )
                .ok();
            Ok(user)
        })
        .await?
    }

    pub async fn get_gateway_by_user_id(&self, user_id: &str) -> Result<Option<Gateway>> {
        let conn = self.conn.clone();
        let user_id = user_id.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let gateway = conn
                .query_row(
                    "SELECT id, user_id, key, name, created_at FROM gateways WHERE user_id = ?",
                    params![&user_id],
                    |row| {
                        Ok(Gateway {
                            id: row.get(0)?,
                            user_id: row.get(1)?,
                            key: row.get(2)?,
                            name: row.get(3)?,
                            created_at: Self::parse_datetime(&row.get::<_, String>(4)?),
                        })
                    },
                )
                .ok();
            Ok(gateway)
        })
        .await?
    }

    pub async fn get_gateway_by_key(&self, key: &str) -> Result<Option<Gateway>> {
        let conn = self.conn.clone();
        let key = key.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let gateway = conn
                .query_row(
                    "SELECT id, user_id, key, name, created_at FROM gateways WHERE key = ?",
                    params![&key],
                    |row| {
                        Ok(Gateway {
                            id: row.get(0)?,
                            user_id: row.get(1)?,
                            key: row.get(2)?,
                            name: row.get(3)?,
                            created_at: Self::parse_datetime(&row.get::<_, String>(4)?),
                        })
                    },
                )
                .ok();
            Ok(gateway)
        })
        .await?
    }

    pub async fn regenerate_gateway_key(&self, gateway_id: &str) -> Result<String> {
        let conn = self.conn.clone();
        let gateway_id = gateway_id.to_string();
        let new_key = Self::generate_key();
        let key_clone = new_key.clone();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            conn.execute(
                "UPDATE gateways SET key = ? WHERE id = ?",
                params![&key_clone, &gateway_id],
            )?;
            Ok(new_key)
        })
        .await?
    }

    pub async fn create_device(&self, gateway_id: &str, name: &str, alias: &str) -> Result<Device> {
        let conn = self.conn.clone();
        let gateway_id = gateway_id.to_string();
        let name = name.to_string();
        let alias = alias.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let id = Uuid::new_v4().to_string();
            let key = Self::generate_key();
            let now = Utc::now();
            let mcp_config = McpConfig::default();

            conn.execute(
                "INSERT INTO devices (id, gateway_id, key, name, alias, mcp_config, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![&id, &gateway_id, &key, &name, &alias, serde_json::to_string(&mcp_config)?, now.to_rfc3339()],
            )?;

            Ok(Device {
                id,
                gateway_id,
                key,
                name,
                alias,
                mcp_config,
                created_at: now,
            })
        })
        .await?
    }

    pub async fn get_device_by_key(&self, key: &str) -> Result<Option<DeviceWithGateway>> {
        let conn = self.conn.clone();
        let key = key.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let result = conn
                .query_row(
                    "SELECT d.id, d.gateway_id, d.key, d.name, d.alias, d.mcp_config, d.created_at, g.key as gateway_key
                     FROM devices d
                     JOIN gateways g ON d.gateway_id = g.id
                     WHERE d.key = ?",
                    params![&key],
                    |row| {
                        let mcp_config_json: String = row.get(5)?;
                        Ok(DeviceWithGateway {
                            device: Device {
                                id: row.get(0)?,
                                gateway_id: row.get(1)?,
                                key: row.get(2)?,
                                name: row.get(3)?,
                                alias: row.get(4)?,
                                mcp_config: serde_json::from_str(&mcp_config_json).unwrap_or_default(),
                                created_at: Self::parse_datetime(&row.get::<_, String>(6)?),
                            },
                            gateway_key: row.get(7)?,
                        })
                    },
                )
                .ok();
            Ok(result)
        })
        .await?
    }

    pub async fn get_devices_by_gateway_id(&self, gateway_id: &str) -> Result<Vec<Device>> {
        let conn = self.conn.clone();
        let gateway_id = gateway_id.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let mut stmt = conn.prepare(
                "SELECT id, gateway_id, key, name, alias, mcp_config, created_at FROM devices WHERE gateway_id = ? ORDER BY created_at DESC",
            )?;

            let devices = stmt
                .query_map(params![&gateway_id], |row| {
                    let mcp_config_json: String = row.get(5)?;
                    Ok(Device {
                        id: row.get(0)?,
                        gateway_id: row.get(1)?,
                        key: row.get(2)?,
                        name: row.get(3)?,
                        alias: row.get(4)?,
                        mcp_config: serde_json::from_str(&mcp_config_json).unwrap_or_default(),
                        created_at: Self::parse_datetime(&row.get::<_, String>(6)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(devices)
        })
        .await?
    }

    pub async fn delete_device(&self, device_id: &str, gateway_id: &str) -> Result<bool> {
        let conn = self.conn.clone();
        let device_id = device_id.to_string();
        let gateway_id = gateway_id.to_string();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let rows = conn.execute(
                "DELETE FROM devices WHERE id = ? AND gateway_id = ?",
                params![&device_id, &gateway_id],
            )?;
            Ok(rows > 0)
        })
        .await?
    }

    pub async fn regenerate_device_key(
        &self,
        device_id: &str,
        gateway_id: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.clone();
        let device_id = device_id.to_string();
        let gateway_id = gateway_id.to_string();
        let new_key = Self::generate_key();
        let key_clone = new_key.clone();

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let rows = conn.execute(
                "UPDATE devices SET key = ? WHERE id = ? AND gateway_id = ?",
                params![&key_clone, &device_id, &gateway_id],
            )?;

            if rows > 0 {
                Ok(Some(new_key))
            } else {
                Ok(None)
            }
        })
        .await?
    }

    pub async fn update_mcp_config(&self, device_id: &str, gateway_id: &str, config: &McpConfig) -> Result<bool> {
        let conn = self.conn.clone();
        let device_id = device_id.to_string();
        let gateway_id = gateway_id.to_string();
        let config_json = serde_json::to_string(config)?;

        task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            let rows = conn.execute(
                "UPDATE devices SET mcp_config = ? WHERE id = ? AND gateway_id = ?",
                params![&config_json, &device_id, &gateway_id],
            )?;
            Ok(rows > 0)
        })
        .await?
    }
}
