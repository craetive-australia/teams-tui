use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Azure CLI public client ID (Pre-authorized first-party Microsoft multi-tenant client)
pub const DEFAULT_PUBLIC_CLIENT_ID: &str = "86a0c6ec-236e-410b-a210-a479b1f15aa3";
pub const DEFAULT_TENANT_ID: &str = "common";

#[derive(Parser, Debug, Clone)]
#[command(
    name = "teams-tui",
    author = "Teams TUI Team",
    version = "0.1.0",
    about = "Terminal User Interface for Microsoft Teams using Microsoft Graph API"
)]
pub struct CliArgs {
    /// Azure Entra ID Client ID (defaults to Microsoft Graph CLI public client ID)
    #[arg(short = 'c', long, env = "TEAMS_CLIENT_ID")]
    pub client_id: Option<String>,

    /// Azure Entra ID Tenant ID (e.g. 'common', 'organizations', or tenant GUID)
    #[arg(short = 't', long, env = "TEAMS_TENANT_ID")]
    pub tenant_id: Option<String>,

    /// Force re-authentication (clears or ignores stored tokens)
    #[arg(long)]
    pub login: bool,

    /// Custom path to config.toml
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Enable verbose debug logging to file
    #[arg(short = 'd', long)]
    pub debug: bool,

    /// Path to debug log file (defaults to teams-tui.log)
    #[arg(long)]
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub client_id: String,
    pub tenant_id: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "default_log_file")]
    pub log_file: String,
}

fn default_poll_interval() -> u64 {
    5
}

fn default_log_file() -> String {
    "teams-tui.log".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_PUBLIC_CLIENT_ID.to_string(),
            tenant_id: DEFAULT_TENANT_ID.to_string(),
            poll_interval_secs: 5,
            debug: false,
            log_file: default_log_file(),
        }
    }
}

impl AppConfig {
    pub fn load(args: &CliArgs) -> Self {
        let config_path = args
            .config
            .clone()
            .unwrap_or_else(Self::default_config_path);

        let file_config: Option<AppConfig> = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|content| toml::from_str(&content).ok())
        } else {
            // Write a template config for the user if it doesn't exist
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let default_cfg = AppConfig::default();
            if let Ok(toml_str) = toml::to_string_pretty(&default_cfg) {
                let _ = fs::write(&config_path, toml_str);
            }
            None
        };

        let mut config = file_config.unwrap_or_default();
        let mut modified = false;

        if let Some(ref cid) = args.client_id {
            config.client_id = cid.clone();
            modified = true;
        }
        if let Some(ref tid) = args.tenant_id {
            config.tenant_id = tid.clone();
            modified = true;
        }
        if args.debug {
            config.debug = true;
        }
        if let Some(ref lf) = args.log_file {
            config.log_file = lf.clone();
            modified = true;
        }

        if modified {
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(toml_str) = toml::to_string_pretty(&config) {
                let _ = fs::write(&config_path, toml_str);
            }
        }

        config
    }

    pub fn default_config_path() -> PathBuf {
        if let Some(dirs) = directories::ProjectDirs::from("com", "microsoft", "teams-tui") {
            dirs.config_dir().join("config.toml")
        } else {
            PathBuf::from("config.toml")
        }
    }
}
