use crate::riot::{normalize_region, AccountProfile, ActiveAccount, RiotManager};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub accounts: Vec<AccountProfile>,
}

#[derive(Clone)]
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

        let mgr = Self {
            profiles_dir,
            config_file,
        };

        mgr.import_external_profiles();
        mgr
    }

    pub fn load_config(&self) -> AppConfig {
        if !self.config_file.exists() {
            return AppConfig::default();
        }

        let content = match fs::read_to_string(&self.config_file) {
            Ok(c) => c,
            Err(_) => return AppConfig::default(),
        };

        let mut cfg: AppConfig = serde_json::from_str(&content).unwrap_or_default();
        for acc in &mut cfg.accounts {
            acc.region = normalize_region(&acc.region);
        }
        cfg
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<(), String> {
        let json_str = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Serialization error: {}", e))?;
        fs::write(&self.config_file, json_str)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }

    pub fn make_safe_id(active: &ActiveAccount) -> String {
        let clean_name = if !active.game_name.is_empty() {
            format!("{}_{}", active.game_name, active.tag_line)
        } else if !active.username.is_empty() {
            active.username.clone()
        } else {
            "account".to_string()
        };

        clean_name
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }

    /// Imports or updates newer valid sessions from %APPDATA%\RiotSwitcher\profiles if present.
    pub fn import_external_profiles(&self) {
        let app_data = match std::env::var("APPDATA") {
            Ok(v) => v,
            Err(_) => return,
        };
        let ext_profiles = PathBuf::from(app_data).join("RiotSwitcher").join("profiles");
        if !ext_profiles.exists() {
            return;
        }

        if let Ok(entries) = fs::read_dir(ext_profiles) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let yml = path.join("RiotGamesPrivateSettings.yaml");
                    if let Some(act) = RiotManager::parse_account_from_yaml(&yml) {
                        let _ = self.save_active_account(&act, None, &yml);
                    }
                }
            }
        }
    }

    /// Backs up whatever valid session is currently in RiotGamesPrivateSettings.yaml before switching.
    pub fn backup_current_active_session(&self, riot: &RiotManager) {
        if let Some(act) = RiotManager::parse_account_from_yaml(&riot.settings_yaml_path) {
            let _ = self.save_active_account(&act, None, &riot.settings_yaml_path);
        }
    }

    pub fn save_active_account(
        &self,
        active: &ActiveAccount,
        custom_alias: Option<&str>,
        source_yaml: &PathBuf,
    ) -> Result<AccountProfile, String> {
        let parsed_src = RiotManager::parse_account_from_yaml(source_yaml)
            .ok_or_else(|| "RiotGamesPrivateSettings.yaml has no valid persistent session (ensure 'Stay signed in' is checked)".to_string())?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let safe_id = Self::make_safe_id(active);
        let yaml_filename = format!("{}.yaml", safe_id);
        let destination_yaml = self.profiles_dir.join(&yaml_filename);

        // Only overwrite existing YAML if source token is at least as new as saved token
        let should_copy_yaml = if destination_yaml.exists() {
            if let Some(existing_acc) = RiotManager::parse_account_from_yaml(&destination_yaml) {
                parsed_src.last_token_time >= existing_acc.last_token_time
                    || parsed_src.last_token_time == 0
            } else {
                true
            }
        } else {
            true
        };

        if should_copy_yaml && source_yaml != &destination_yaml {
            fs::copy(source_yaml, &destination_yaml)
                .map_err(|e| format!("Failed to copy YAML session: {}", e))?;
        }

        // Also backup RiotClientSettings.yaml if source_yaml is the active Riot Client file
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let active_rc_yaml = PathBuf::from(&local_app_data)
                .join("Riot Games")
                .join("Riot Client")
                .join("Data")
                .join("RiotGamesPrivateSettings.yaml");
            if source_yaml == &active_rc_yaml {
                let rc_settings = PathBuf::from(&local_app_data)
                    .join("Riot Games")
                    .join("Riot Client")
                    .join("Config")
                    .join("RiotClientSettings.yaml");
                if rc_settings.exists() {
                    let dest_client = self.profiles_dir.join(format!("{}_client.yaml", safe_id));
                    let _ = fs::copy(rc_settings, dest_client);
                }
            }
        }

        let alias = if let Some(a) = custom_alias {
            if !a.trim().is_empty() {
                a.trim().to_string()
            } else {
                active.display_name()
            }
        } else {
            active.display_name()
        };

        let mut config = self.load_config();
        let existing_created = config
            .accounts
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(&safe_id))
            .map(|a| a.created_at)
            .unwrap_or(now);

        let profile = AccountProfile {
            id: safe_id.clone(),
            alias,
            game_name: active.game_name.clone(),
            tag_line: active.tag_line.clone(),
            username: active.username.clone(),
            region: normalize_region(&active.region),
            yaml_filename,
            created_at: existing_created,
            last_used: now,
        };

        if let Some(pos) = config
            .accounts
            .iter()
            .position(|a| a.id.eq_ignore_ascii_case(&safe_id))
        {
            config.accounts[pos] = profile.clone();
        } else {
            config.accounts.push(profile.clone());
        }
        self.save_config(&config)?;

        Ok(profile)
    }

    #[allow(dead_code)]
    pub fn update_saved_account_session(
        &self,
        active: &ActiveAccount,
        source_yaml: &PathBuf,
    ) -> Result<(), String> {
        let _ = self.save_active_account(active, None, source_yaml)?;
        Ok(())
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
            .position(|a| a.id.eq_ignore_ascii_case(profile_id))
            .ok_or_else(|| format!("Account profile '{}' not found", profile_id))?;

        let profile = &mut config.accounts[index];
        let saved_yaml = self.profiles_dir.join(&profile.yaml_filename);

        if !saved_yaml.exists() {
            return Err(format!("Saved session file not found: {:?}", saved_yaml));
        }

        // Validate that the saved profile YAML still contains a valid refresh_token
        if RiotManager::parse_account_from_yaml(&saved_yaml).is_none() {
            return Err(format!(
                "Session for {} expired or invalid. Click '+ Add Account' to re-login.",
                profile.game_name
            ));
        }

        if let Some(parent) = target_yaml.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::copy(&saved_yaml, target_yaml)
            .map_err(|e| format!("Failed to restore session YAML: {}", e))?;

        // Also restore paired RiotClientSettings.yaml if available
        let saved_client_settings = self.profiles_dir.join(format!("{}_client.yaml", profile.id));
        if saved_client_settings.exists() {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let target_client_settings = PathBuf::from(&local_app_data)
                    .join("Riot Games")
                    .join("Riot Client")
                    .join("Config")
                    .join("RiotClientSettings.yaml");
                let _ = fs::copy(&saved_client_settings, target_client_settings);
            }
        }

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
        if let Some(idx) = config
            .accounts
            .iter()
            .position(|a| a.id.eq_ignore_ascii_case(profile_id))
        {
            let acc = config.accounts.remove(idx);
            let saved_yaml = self.profiles_dir.join(&acc.yaml_filename);
            let _ = fs::remove_file(saved_yaml);
            let saved_client = self.profiles_dir.join(format!("{}_client.yaml", acc.id));
            let _ = fs::remove_file(saved_client);
            self.save_config(&config)?;
            Ok(())
        } else {
            Err("Account not found".to_string())
        }
    }
}
