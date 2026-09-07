use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const KEYRING_SERVICE: &str = "teams-tui";
const KEYRING_USER: &str = "ms-graph-tokens";

pub const SCOPES: &[&str] = &[
    "User.Read",
    "User.ReadBasic.All",
    "People.Read",
    "Chat.ReadWrite",
    "ChatMessage.Read",
    "ChatMessage.Send",
    "ChannelMessage.Read.All",
    "ChannelMessage.Send",
    "Team.ReadBasic.All",
    "Channel.ReadBasic.All",
    "Presence.Read",
    "Presence.ReadWrite",
    "offline_access",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenStore {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>, // Unix timestamp in seconds
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[allow(dead_code)]
    expires_in: u64,
    interval: Option<u64>,
    message: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

pub struct AuthManager {
    client: Client,
    client_id: String,
    tenant_id: String,
}

impl AuthManager {
    pub fn new(client_id: String, tenant_id: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            client_id,
            tenant_id,
        }
    }

    /// Primary entry point: Get valid access token (using cache, refresh, or interactive device login)
    pub async fn get_access_token(&self, force_login: bool) -> Result<String, Box<dyn std::error::Error>> {
        if !force_login {
            if let Some(tokens) = self.load_tokens() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // If token is still valid for at least 3 minutes, use it
                if let Some(exp) = tokens.expires_at {
                    if exp > now + 180 {
                        return Ok(tokens.access_token);
                    }
                }

                // If expired or expiring soon, attempt refresh
                if let Some(ref refresh_tok) = tokens.refresh_token {
                    match self.refresh_token(refresh_tok).await {
                        Ok(new_tokens) => {
                            self.save_tokens(&new_tokens);
                            return Ok(new_tokens.access_token);
                        }
                        Err(e) => {
                            eprintln!("Token refresh failed: {}. Falling back to device login...", e);
                        }
                    }
                }
            }
        }

        // Perform Device Code Login
        let tokens = self.device_login().await?;
        self.save_tokens(&tokens);
        Ok(tokens.access_token)
    }

    /// Initiate OAuth2 Device Code Flow
    async fn device_login(&self) -> Result<TokenStore, Box<dyn std::error::Error>> {
        let device_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode",
            self.tenant_id
        );

        let scopes_joined = SCOPES.join(" ");
        let params = [
            ("client_id", self.client_id.as_str()),
            ("scope", scopes_joined.as_str()),
        ];

        let resp = self
            .client
            .post(&device_url)
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(format!("Device code request failed: {}", err_text).into());
        }

        let device_code_data: DeviceCodeResponse = resp.json().await?;

        println!("\n+-------------------------------------------------------+");
        println!("|           MICROSOFT TEAMS AUTHENTICATION             |");
        println!("+-------------------------------------------------------+");
        println!("  {}", device_code_data.message);
        println!();
        println!("  1. Open:  {}", device_code_data.verification_uri);
        println!("  2. Code:  {}", device_code_data.user_code);
        println!("+-------------------------------------------------------+");
        println!("Waiting for sign-in in browser...\n");

        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        );

        let mut poll_interval = Duration::from_secs(device_code_data.interval.unwrap_or(5).max(5));

        loop {
            sleep(poll_interval).await;

            let poll_params = [
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.client_id.as_str()),
                ("device_code", device_code_data.device_code.as_str()),
            ];

            let res = self
                .client
                .post(&token_url)
                .form(&poll_params)
                .send()
                .await?;

            let body: TokenResponse = res.json().await?;

            if let Some(ref err) = body.error {
                match err.as_str() {
                    "authorization_pending" => {
                        // User has not finished entering code yet; continue polling
                        continue;
                    }
                    "slow_down" => {
                        poll_interval += Duration::from_secs(5);
                        continue;
                    }
                    "expired_token" => {
                        return Err("Device code expired. Please rerun teams-tui.".into());
                    }
                    "access_denied" => {
                        return Err("Authorization was cancelled or denied by the user.".into());
                    }
                    other => {
                        let desc = body.error_description.unwrap_or_default();
                        return Err(format!("Login failed: {} - {}", other, desc).into());
                    }
                }
            }

            if let Some(access_tok) = body.access_token {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let expires_at = body.expires_in.map(|secs| now + secs);

                println!("Successfully authenticated!");
                return Ok(TokenStore {
                    access_token: access_tok,
                    refresh_token: body.refresh_token,
                    expires_at,
                });
            }

            return Err("Unexpected empty token response from Microsoft Entra.".into());
        }
    }

    /// Refresh access token using stored refresh token
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenStore, Box<dyn std::error::Error>> {
        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        );

        let scopes_joined = SCOPES.join(" ");
        let params = [
            ("grant_type", "refresh_token"),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", scopes_joined.as_str()),
        ];

        let resp = self
            .client
            .post(&token_url)
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let err_text = resp.text().await?;
            return Err(format!("Token refresh request failed: {}", err_text).into());
        }

        let body: TokenResponse = resp.json().await?;

        if let Some(access_tok) = body.access_token {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let expires_at = body.expires_in.map(|secs| now + secs);

            Ok(TokenStore {
                access_token: access_tok,
                refresh_token: body.refresh_token.or_else(|| Some(refresh_token.to_string())),
                expires_at,
            })
        } else {
            Err("No access token in refresh response".into())
        }
    }

    /// Load token from OS Keyring or fallback file
    fn load_tokens(&self) -> Option<TokenStore> {
        // Try OS Keyring
        if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            if let Ok(json_str) = entry.get_password() {
                if let Ok(tokens) = serde_json::from_str::<TokenStore>(&json_str) {
                    return Some(tokens);
                }
            }
        }

        // Fallback: local session file
        let fallback_path = Self::fallback_token_path();
        if fallback_path.exists() {
            if let Ok(content) = fs::read_to_string(&fallback_path) {
                if let Ok(tokens) = serde_json::from_str::<TokenStore>(&content) {
                    return Some(tokens);
                }
            }
        }

        None
    }

    /// Save token to OS Keyring and fallback file securely
    fn save_tokens(&self, tokens: &TokenStore) {
        if let Ok(json_str) = serde_json::to_string(tokens) {
            // Try OS Keyring first
            if let Ok(entry) = Entry::new(KEYRING_SERVICE, KEYRING_USER) {
                if entry.set_password(&json_str).is_ok() {
                    return; // Avoid writing plain text fallback if keyring succeeded
                }
            }

            // Fallback: local session file
            let fallback_path = Self::fallback_token_path();
            if let Some(parent) = fallback_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                let _ = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&fallback_path)
                    .and_then(|mut f| {
                        use std::io::Write;
                        f.write_all(json_str.as_bytes())
                    });
            }
            #[cfg(not(unix))]
            {
                let _ = std::fs::write(&fallback_path, json_str);
            }
        }
    }

    fn fallback_token_path() -> PathBuf {
        if let Some(dirs) = directories::ProjectDirs::from("com", "microsoft", "teams-tui") {
            dirs.data_local_dir().join("session.json")
        } else {
            PathBuf::from(".teams_session.json")
        }
    }
}
