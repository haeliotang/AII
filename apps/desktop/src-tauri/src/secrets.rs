//! API-key storage.
//!
//! The API key is never written to SQLite. Production uses the macOS Keychain
//! through the system `security` command (and the env fallback for headless
//! verification); other platforms return an explicit unsupported error until a
//! native store is added. Tests inject an in-memory store.

use std::process::Command;

pub trait SecretStore: Send + Sync {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String>;
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String>;
    fn delete(&self, service: &str, account: &str) -> Result<(), String>;
}

pub struct SystemSecretStore;

impl SystemSecretStore {
    fn run(args: &[&str]) -> Result<std::process::Output, String> {
        Command::new("security").args(args).output().map_err(|error| format!("system credential store: {error}"))
    }
}

impl SecretStore for SystemSecretStore {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        if std::env::var("POIEWEAVE_INFERENCE_API_KEY").map(|stored| !stored.is_empty()).unwrap_or(false) {
            // Explicit environment override is for headless deployments; it is
            // read-only and cannot be overwritten from the UI.
            return Err("inference key is managed by POIEWEAVE_INFERENCE_API_KEY".into());
        }
        if !cfg!(target_os = "macos") {
            return Err("no native credential store on this platform yet".into());
        }
        let output = Self::run(&["add-generic-password", "-U", "-s", service, "-a", account, "-w", value])?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!("macOS Keychain refused the API key: {}", String::from_utf8_lossy(&output.stderr).trim()))
        }
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        if let Ok(value) = std::env::var("POIEWEAVE_INFERENCE_API_KEY") {
            if !value.is_empty() {
                return Ok(Some(value));
            }
        }
        if !cfg!(target_os = "macos") {
            return Ok(None);
        }
        let output = Self::run(&["find-generic-password", "-s", service, "-a", account, "-w"])?;
        if !output.status.success() {
            return Ok(None);
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if value.is_empty() { None } else { Some(value) })
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        if std::env::var("POIEWEAVE_INFERENCE_API_KEY").map(|stored| !stored.is_empty()).unwrap_or(false) {
            return Err("inference key is managed by POIEWEAVE_INFERENCE_API_KEY".into());
        }
        if !cfg!(target_os = "macos") {
            return Ok(());
        }
        let output = Self::run(&["delete-generic-password", "-s", service, "-a", account])?;
        if output.status.success() || String::from_utf8_lossy(&output.stderr).contains("could not be found") {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

#[cfg(test)]
pub struct MemorySecretStore {
    values: std::sync::Mutex<std::collections::BTreeMap<(String, String), String>>,
}

#[cfg(test)]
impl MemorySecretStore {
    pub fn new() -> MemorySecretStore {
        MemorySecretStore { values: std::sync::Mutex::new(std::collections::BTreeMap::new()) }
    }
}

#[cfg(test)]
impl SecretStore for MemorySecretStore {
    fn set(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        self.values.lock().unwrap().insert((service.to_string(), account.to_string()), value.to_string());
        Ok(())
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        Ok(self.values.lock().unwrap().get(&(service.to_string(), account.to_string())).cloned())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        self.values.lock().unwrap().remove(&(service.to_string(), account.to_string()));
        Ok(())
    }
}
