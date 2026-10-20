use crate::riot::{AccountProfile, ActiveAccount};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub accounts: Vec<AccountProfile>,
}

pub struct StorageManager {
    pub profiles_dir: PathBuf,
    pub config_file: PathBuf,
}

impl StorageManager {
    pub fn new() -> Self {
        let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        let base_dir = PathBuf::from(app_data).join("RiotAccountSwitcher");
        let profiles_dir = base_dir.join("profiles");
        let config_file = base_dir.join("accounts.json");

        let _ = fs::create_dir_all(&profiles_dir);

        Self {
            profiles_dir,
            config_file,
        }
    }

    pub fn load_config(&self) -> AppConfig {
        if !self.config_file.exists() {
            return AppConfig::default();
        }

        let content = match fs::read_to_string(&self.config_file) {
            Ok(c) => c,
            Err(_) => return AppConfig::default(),
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), String> {
        let json_str = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Serialization error: {}", e))?;
        fs::write(&self.config_file, json_str)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }

    pub fn save_active_account(
        &self,
        active: &ActiveAccount,
        custom_alias: Option<&str>,
        source_yaml: &PathBuf,
    ) -> Result<AccountProfile, String> {
        if !source_yaml.exists() {
            return Err("RiotGamesPrivateSettings.yaml does not exist or not logged in".to_string());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Create a unique clean ID
        let clean_name = if !active.game_name.is_empty() {
            format!("{}_{}", active.game_name, active.tag_line)
        } else if !active.username.is_empty() {
            active.username.clone()
        } else {
            format!("account_{}", now)
        };

        let safe_id: String = clean_name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();

        let yaml_filename = format!("{}.yaml", safe_id);
        let destination_yaml = self.profiles_dir.join(&yaml_filename);

        fs::copy(source_yaml, &destination_yaml)
            .map_err(|e| format!("Failed to copy YAML session: {}", e))?;

        let alias = if let Some(a) = custom_alias {
            if !a.trim().is_empty() {
                a.trim().to_string()
            } else {
                active.display_name()
            }
        } else {
            active.display_name()
        };

        let profile = AccountProfile {
            id: safe_id.clone(),
            alias,
            game_name: active.game_name.clone(),
            tag_line: active.tag_line.clone(),
            username: active.username.clone(),
            region: active.region.clone(),
            yaml_filename,
            created_at: now,
            last_used: now,
        };

        let mut config = self.load_config();
        // Replace existing account with same ID if exists
        config.accounts.retain(|a| a.id != safe_id);
        config.accounts.push(profile.clone());
        self.save_config(&config)?;

        Ok(profile)
    }

    pub fn apply_profile(
        &self,
        profile_id: &str,
        target_yaml: &PathBuf,
    ) -> Result<AccountProfile, String> {
        let mut config = self.load_config();
        let index = config
            .accounts
            .iter()
            .position(|a| a.id == profile_id)
            .ok_or_else(|| format!("Account profile with id '{}' not found", profile_id))?;

        let profile = &mut config.accounts[index];
        let saved_yaml = self.profiles_dir.join(&profile.yaml_filename);

        if !saved_yaml.exists() {
            return Err(format!("Saved session file not found: {:?}", saved_yaml));
        }

        // Ensure parent directory exists
        if let Some(parent) = target_yaml.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::copy(&saved_yaml, target_yaml)
            .map_err(|e| format!("Failed to restore session YAML: {}", e))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        profile.last_used = now;
        let updated_profile = profile.clone();
        self.save_config(&config)?;

        Ok(updated_profile)
    }

    pub fn delete_profile(&self, profile_id: &str) -> Result<(), String> {
        let mut config = self.load_config();
        if let Some(idx) = config.accounts.iter().position(|a| a.id == profile_id) {
            let acc = config.accounts.remove(idx);
            let saved_yaml = self.profiles_dir.join(&acc.yaml_filename);
            let _ = fs::remove_file(saved_yaml);
            self.save_config(&config)?;
            Ok(())
        } else {
            Err("Account not found".to_string())
        }
    }
}
