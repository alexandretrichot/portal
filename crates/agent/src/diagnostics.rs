use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub name: String,
    pub status: PermissionStatus,
    pub settings_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostics {
    pub os: String,
    pub permissions: Vec<Permission>,
}

impl Diagnostics {
    pub fn check() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::check_macos()
        }
        #[cfg(target_os = "linux")]
        {
            Self::check_linux()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Self {
                os: std::env::consts::OS.to_string(),
                permissions: vec![],
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn check_macos() -> Self {
        Self {
            os: "macos".to_string(),
            permissions: vec![
                check_full_disk_access(),
                check_accessibility(),
            ],
        }
    }

    #[cfg(target_os = "linux")]
    fn check_linux() -> Self {
        Self {
            os: "linux".to_string(),
            permissions: vec![], // Linux n'a pas de permissions système équivalentes
        }
    }

    pub fn all_granted(&self) -> bool {
        self.permissions.iter().all(|p| {
            p.status == PermissionStatus::Granted || p.status == PermissionStatus::NotApplicable
        })
    }

    pub fn open_settings(&self, permission_name: &str) -> bool {
        if let Some(perm) = self.permissions.iter().find(|p| p.name == permission_name) {
            if let Some(url) = &perm.settings_url {
                return std::process::Command::new("open")
                    .arg(url)
                    .spawn()
                    .is_ok();
            }
        }
        false
    }
}

#[cfg(target_os = "macos")]
fn check_full_disk_access() -> Permission {
    let home = dirs::home_dir();

    let protected_paths = [
        home.as_ref().map(|h| h.join("Library/Messages/chat.db")),
        home.as_ref().map(|h| h.join("Library/Safari/History.db")),
        home.as_ref().map(|h| h.join("Library/Mail/V10")),
    ];

    let status = protected_paths
        .iter()
        .flatten()
        .find(|path| path.exists())
        .map(|path| {
            match std::fs::File::open(path) {
                Ok(_) => PermissionStatus::Granted,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    PermissionStatus::Denied
                }
                Err(_) => PermissionStatus::Unknown,
            }
        })
        .unwrap_or(PermissionStatus::Unknown);

    Permission {
        name: "full_disk_access".to_string(),
        status,
        settings_url: Some(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles".to_string(),
        ),
    }
}

#[cfg(target_os = "macos")]
fn check_accessibility() -> Permission {
    Permission {
        name: "accessibility".to_string(),
        status: macos_accessibility_check(),
        settings_url: Some(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
                .to_string(),
        ),
    }
}

#[cfg(target_os = "macos")]
fn macos_accessibility_check() -> PermissionStatus {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    if unsafe { AXIsProcessTrusted() } {
        PermissionStatus::Granted
    } else {
        PermissionStatus::Denied
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics() {
        let diag = Diagnostics::check();
        println!("OS: {}", diag.os);
        for perm in &diag.permissions {
            println!("  {}: {:?}", perm.name, perm.status);
        }
    }
}
