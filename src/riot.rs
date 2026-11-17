use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
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
    pub sub: String,
    pub last_token_time: u64,
    pub raw_yaml_path: PathBuf,
}

impl ActiveAccount {
    pub fn display_name(&self) -> String {
        if self.game_name.is_empty() && self.username.is_empty() {
            "No Active Account".to_string()
        } else if self.tag_line.is_empty() {
            if self.username.is_empty() {
                self.game_name.clone()
            } else {
                format!("{} ({})", self.game_name, self.username)
            }
        } else if self.username.is_empty() {
            format!("{} #{}", self.game_name, self.tag_line)
        } else {
            format!("{} #{} ({})", self.game_name, self.tag_line, self.username)
        }
    }
}

pub fn normalize_region(raw: &str) -> String {
    let up = raw.trim().to_uppercase();
    match up.as_str() {
        "EUW1" | "EUW" => "EUW".to_string(),
        "EUN1" | "EUNE" => "EUNE".to_string(),
        "NA1" | "NA" | "USA" => "NA".to_string(),
        "ME1" | "ME" | "EGY" | "SAU" | "ARE" => "ME".to_string(),
        "TR1" | "TR" | "TUR" => "TR".to_string(),
        "BR1" | "BR" | "BRA" => "BR".to_string(),
        "LA1" | "LAN" => "LAN".to_string(),
        "LA2" | "LAS" => "LAS".to_string(),
        "OC1" | "OCE" | "AUS" => "OCE".to_string(),
        "JP1" | "JP" | "JPN" => "JP".to_string(),
        "KR" | "KOR" => "KR".to_string(),
        "RU" | "RUS" => "RU".to_string(),
        "SG2" | "SG" | "SGP" => "SG".to_string(),
        "TW2" | "TW" | "TWN" => "TW".to_string(),
        "VN2" | "VN" | "VNM" => "VN".to_string(),
        "PH2" | "PH" | "PHL" => "PH".to_string(),
        "" | "UNKNOWN" => "GLB".to_string(),
        other => other.to_string(),
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct RiotManager {
    pub settings_yaml_path: PathBuf,
    pub client_settings_yaml_path: PathBuf,
    pub riot_client_exe_path: PathBuf,
}

impl RiotManager {
    pub fn new() -> Self {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let rc_root = PathBuf::from(&local_app_data)
            .join("Riot Games")
            .join("Riot Client");

        let settings_yaml_path = rc_root.join("Data").join("RiotGamesPrivateSettings.yaml");
        let client_settings_yaml_path = rc_root.join("Config").join("RiotClientSettings.yaml");
        let riot_client_exe_path = Self::find_riot_client_exe();

        Self {
            settings_yaml_path,
            client_settings_yaml_path,
            riot_client_exe_path,
        }
    }

    fn find_riot_client_exe() -> PathBuf {
        let installs_json = PathBuf::from(r"C:\ProgramData\Riot Games\RiotClientInstalls.json");
        if installs_json.exists() {
            if let Ok(content) = std::fs::read_to_string(&installs_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(rc_path) = json
                        .get("rc_default")
                        .or_else(|| json.get("rc_live"))
                        .and_then(|v| v.as_str())
                    {
                        let p = PathBuf::from(rc_path);
                        if p.exists() {
                            return p;
                        }
                    }
                }
            }
        }

        let standard_path = PathBuf::from(r"C:\Riot Games\Riot Client\RiotClientServices.exe");
        if standard_path.exists() {
            return standard_path;
        }

        if let Ok(program_files) = std::env::var("ProgramFiles") {
            let p = PathBuf::from(program_files).join("Riot Games/Riot Client/RiotClientServices.exe");
            if p.exists() {
                return p;
            }
        }

        standard_path
    }

    pub fn get_active_account(&self) -> Option<ActiveAccount> {
        Self::parse_account_from_yaml(&self.settings_yaml_path)
    }

    /// Parses and validates a RiotGamesPrivateSettings.yaml file.
    /// Returns Some(ActiveAccount) ONLY if both id_token and a valid persistent refresh_token exist.
    pub fn parse_account_from_yaml(yaml_path: &Path) -> Option<ActiveAccount> {
        if !yaml_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(yaml_path).ok()?;

        // Must contain a valid persistent refresh_token (not null or empty)
        let refresh_token = Self::extract_yaml_value(&content, "refresh_token:")?;
        if refresh_token.is_empty() || refresh_token == "null" || refresh_token.len() < 30 {
            return None;
        }

        let id_token = Self::extract_yaml_value(&content, "id_token:")?;
        if id_token.is_empty() || id_token == "null" {
            return None;
        }

        let last_token_time = Self::extract_yaml_value(&content, "last_token_creation_time:")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let (game_name, tag_line, username, region, sub, jwt_iat_ms) =
            Self::parse_jwt_payload(&id_token)?;

        if game_name.is_empty() && username.is_empty() {
            return None;
        }

        Some(ActiveAccount {
            game_name,
            tag_line,
            username,
            region,
            sub,
            last_token_time: if last_token_time > 0 {
                last_token_time
            } else {
                jwt_iat_ms
            },
            raw_yaml_path: yaml_path.to_path_buf(),
        })
    }

    pub fn extract_yaml_value(content: &str, key: &str) -> Option<String> {
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

    fn parse_jwt_payload(jwt: &str) -> Option<(String, String, String, String, String, u64)> {
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

        let decoded_bytes = URL_SAFE_NO_PAD
            .decode(padded.trim_end_matches('='))
            .ok()
            .or_else(|| base64::engine::general_purpose::STANDARD.decode(&padded).ok())?;

        let json_val: serde_json::Value = serde_json::from_slice(&decoded_bytes).ok()?;

        let sub = json_val
            .get("sub")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

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

        let raw_region = json_val
            .get("lol_region")
            .and_then(|r| r.as_array())
            .and_then(|arr| {
                arr.iter()
                    .find(|x| x.get("active").and_then(|a| a.as_bool()).unwrap_or(false))
                    .or_else(|| arr.first())
            })
            .and_then(|item| item.get("cpid").or_else(|| item.get("pid")))
            .and_then(|p| p.as_str())
            .or_else(|| {
                json_val
                    .get("lol")
                    .and_then(|l| l.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|item| item.get("cpid").or_else(|| item.get("pid")))
                    .and_then(|p| p.as_str())
            })
            .or_else(|| json_val.get("login_country").and_then(|c| c.as_str()))
            .or_else(|| json_val.get("country").and_then(|c| c.as_str()))
            .unwrap_or("EUW");

        let region = normalize_region(raw_region);

        let iat_ms = json_val
            .get("iat")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            * 1000;

        Some((game_name, tag_line, username, region, sub, iat_ms))
    }

    pub fn is_riot_client_running() -> bool {
        if let Ok(out) = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq RiotClientServices.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.to_lowercase().contains("riotclientservices.exe") {
                return true;
            }
        }
        if let Ok(out) = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq Riot Client.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.to_lowercase().contains("riot client.exe") {
                return true;
            }
        }
        false
    }

    pub fn kill_riot_processes() {
        let processes = [
            "Riot Client.exe",
            "RiotClientUx.exe",
            "RiotClientUxRender.exe",
            "RiotClientServices.exe",
            "RiotClientCrashHandler.exe",
            "LeagueClient.exe",
            "LeagueClientUx.exe",
            "LeagueClientUxRender.exe",
            "VALORANT.exe",
            "VALORANT-Win64-Shipping.exe",
        ];

        let mut args = vec!["/F".to_string(), "/T".to_string()];
        for proc in processes {
            args.push("/IM".to_string());
            args.push(proc.to_string());
        }

        let _ = Command::new("taskkill")
            .args(&args)
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        // Wait until Riot Client processes have completely exited and released file locks
        for _ in 0..18 {
            if !Self::is_riot_client_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Extra grace delay for OS file handle flush
        std::thread::sleep(std::time::Duration::from_millis(250));

        // Clean up any stale lockfiles and shutdown state
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let rc_root = PathBuf::from(&local_app_data)
                .join("Riot Games")
                .join("Riot Client");
            let _ = std::fs::remove_file(rc_root.join("Config").join("lockfile"));
            let _ = std::fs::remove_file(rc_root.join("Data").join("ShutdownData.yaml"));
        }
        let _ = std::fs::remove_file(r"C:\ProgramData\Riot Games\Metadata\Riot Client\lockfile");
    }

    /// Clears only the active RSO authorization while preserving the machine's tdid cookie.
    pub fn prepare_clean_login(&self) {
        Self::kill_riot_processes();

        // Extract existing rso-authenticator block if present so tdid cookie is preserved
        let mut rso_block = String::new();
        if let Ok(existing) = std::fs::read_to_string(&self.settings_yaml_path) {
            if let Some(idx) = existing.find("rso-authenticator:") {
                rso_block = existing[idx..].to_string();
            }
        }

        let clean_yaml = if !rso_block.is_empty() {
            format!(
                "psl:\n    authorization:\n        riot-client: null\n{}",
                rso_block
            )
        } else {
            "psl:\n    authorization:\n        riot-client: null\n".to_string()
        };

        if let Some(parent) = self.settings_yaml_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.settings_yaml_path, clean_yaml);
    }

    pub fn launch(&self, product: &str) -> Result<(), String> {
        if !self.riot_client_exe_path.exists() {
            return Err(format!(
                "RiotClientServices.exe not found at {:?}",
                self.riot_client_exe_path
            ));
        }

        let mut cmd = Command::new(&self.riot_client_exe_path);
        cmd.creation_flags(CREATE_NO_WINDOW);

        match product {
            "league" => {
                cmd.args([
                    "--launch-product=league_of_legends",
                    "--launch-patchline=live",
                ]);
            }
            "valorant" => {
                cmd.args(["--launch-product=valorant", "--launch-patchline=live"]);
            }
            _ => {
                // Launch Riot Client directly (matches official desktop shortcut)
            }
        }

        cmd.spawn()
            .map_err(|e| format!("Failed to launch Riot Client: {}", e))?;
        Ok(())
    }
}
