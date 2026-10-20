use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountProfile {
    pub id: String,
    pub alias: String,
    pub game_name: String,
    pub tag_line: String,
    pub username: String,
    pub region: String,
    pub yaml_filename: String,
    pub created_at: u64,
    pub last_used: u64,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ActiveAccount {
    pub game_name: String,
    pub tag_line: String,
    pub username: String,
    pub region: String,
    pub raw_yaml_path: PathBuf,
}

impl ActiveAccount {
    pub fn display_name(&self) -> String {
        if self.game_name.is_empty() && self.username.is_empty() {
            "No Active Account".to_string()
        } else if self.tag_line.is_empty() {
            format!("{} ({})", self.game_name, self.username)
        } else {
            format!("{} #{} ({})", self.game_name, self.tag_line, self.username)
        }
    }
}

pub struct RiotManager {
    pub settings_yaml_path: PathBuf,
    pub riot_client_exe_path: PathBuf,
}

impl RiotManager {
    pub fn new() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let settings_yaml_path = PathBuf::from(&local_app_data)
            .join("Riot Games")
            .join("Riot Client")
            .join("Data")
            .join("RiotGamesPrivateSettings.yaml");

        let riot_client_exe_path = Self::find_riot_client_exe();

        Self {
            settings_yaml_path,
            riot_client_exe_path,
        }
    }

    fn find_riot_client_exe() -> PathBuf {
        // 1. Check official Riot Client installs registry file in ProgramData
        let installs_json = PathBuf::from(r"C:\ProgramData\Riot Games\RiotClientInstalls.json");
        if installs_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&installs_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(rc_path) = json.get("rc_default").or_else(|| json.get("rc_live")).and_then(|v| v.as_str()) {
                        let p = PathBuf::from(rc_path);
                        if p.exists() {
                            return p;
                        }
                    }
                }
            }
        }

        // 2. Standard location
        let standard_path = PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe");
        if standard_path.exists() {
            return standard_path;
        }

        // 3. Program Files fallback
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            let p = PathBuf::from(program_files).join("Riot Games/Riot Client/RiotClientServices.exe");
            if p.exists() {
                return p;
            }
        }

        standard_path
    }

    pub fn get_active_account(&self) -> Option<ActiveAccount> {
        if !self.settings_yaml_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&self.settings_yaml_path).ok()?;
        let id_token = Self::extract_yaml_value(&content, "id_token:")?;
        let (game_name, tag_line, username, region) = Self::parse_jwt_payload(&id_token)?;

        Some(ActiveAccount {
            game_name,
            tag_line,
            username,
            region,
            raw_yaml_path: self.settings_yaml_path.clone(),
        })
    }

    fn extract_yaml_value(content: &str, key: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(key) {
                let val = trimmed[key.len()..].trim();
                let clean_val = val.trim_matches('"').trim_matches('\'');
                return Some(clean_val.to_string());
            }
        }
        None
    }

    fn parse_jwt_payload(jwt: &str) -> Option<(String, String, String, String)> {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        let payload_b64 = parts[1];
        let mut padded = payload_b64.to_string();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }

        let decoded_bytes = URL_SAFE_NO_PAD.decode(padded.trim_end_matches('=')).ok()
            .or_else(|| base64::engine::general_purpose::STANDARD.decode(&padded).ok())?;

        let json_val: serde_json::Value = serde_json::from_slice(&decoded_bytes).ok()?;

        let game_name = json_val
            .get("acct")
            .and_then(|a| a.get("game_name"))
            .and_then(|g| g.as_str())
            .unwrap_or("")
            .to_string();

        let tag_line = json_val
            .get("acct")
            .and_then(|a| a.get("tag_line"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let username = json_val
            .get("lol")
            .and_then(|l| l.as_array())
            .and_then(|arr| arr.first())
            .and_then(|item| item.get("uname"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let region = json_val
            .get("lol_region")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.iter().find(|x| x.get("active").and_then(|a| a.as_bool()).unwrap_or(false)))
            .and_then(|item| item.get("cpid").or_else(|| item.get("pid")))
            .and_then(|p| p.as_str())
            .or_else(|| {
                json_val.get("lol")
                    .and_then(|l| l.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("cpid").or_else(|| item.get("pid")))
                    .and_then(|p| p.as_str())
            })
            .unwrap_or("UNKNOWN")
            .to_string();

        Some((game_name, tag_line, username, region))
    }

    pub fn kill_riot_processes() {
        let processes = [
            "LeagueClient.exe",
            "LeagueClientUx.exe",
            "LeagueClientUxRender.exe",
            "VALORANT.exe",
            "VALORANT-Win64-Shipping.exe",
            "RiotClientUx.exe",
            "RiotClientUxRender.exe",
            "RiotClientServices.exe",
            "RiotClientCrashHandler.exe",
        ];

        let mut args = vec!["/F".to_string()];
        for proc in processes {
            args.push("/IM".to_string());
            args.push(proc.to_string());
        }

        // Execute taskkill completely silently in super-headless mode (NO CMD window popup)
        let _ = Command::new("taskkill")
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        std::thread::sleep(std::time::Duration::from_millis(600));
    }

    pub fn launch(&self, product: &str) -> Result<(), String> {
        if !self.riot_client_exe_path.exists() {
            return Err(format!("RiotClientServices.exe not found at {:?}", self.riot_client_exe_path));
        }

        let mut cmd = Command::new(&self.riot_client_exe_path);
        cmd.creation_flags(CREATE_NO_WINDOW);

        match product {
            "league" => {
                cmd.args(["--launch-product=league_of_legends", "--launch-patchline=live"]);
            }
            "valorant" => {
                cmd.args(["--launch-product=valorant", "--launch-patchline=live"]);
            }
            _ => {
                cmd.args(["--launch-product=riot_client", "--launch-patchline=live"]);
            }
        }

        cmd.spawn().map_err(|e| format!("Failed to launch Riot Client: {}", e))?;
        Ok(())
    }
}
