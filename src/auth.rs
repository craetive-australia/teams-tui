use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
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
    allow_insecure_token_file: bool,
    cached_tokens: Arc<RwLock<Option<TokenStore>>>,
}

impl AuthManager {
    pub fn new(client_id: String, tenant_id: String, allow_insecure_token_file: bool) -> Result<Self, String> {
        Self::validate_tenant_id(&tenant_id)?;

        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            client_id,
            tenant_id,
            allow_insecure_token_file,
            cached_tokens: Arc::new(RwLock::new(None)),
        })
    }

    fn validate_tenant_id(tenant_id: &str) -> Result<(), String> {
        let is_valid = !tenant_id.is_empty()
            && tenant_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
        if !is_valid {
            return Err(format!(
                "Invalid tenant_id '{tenant_id}': Must contain only alphanumeric characters, hyphens, and periods."
            ));
        }
        Ok(())
    }

    /// Primary entry point: Get valid access token (using cache, refresh, or interactive device login)
    pub async fn get_access_token(&self, force_login: bool) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        if !force_login {
            // Check memory cache first
            {
                let lock = self.cached_tokens.read().await;
                if let Some(ref tokens) = *lock {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs());
                    if let Some(exp) = tokens.expires_at {
                        if exp > now + 180 {
                            return Ok(tokens.access_token.clone());
                        }
                    }
                }
            }

            // Check persistent store
            if let Some(tokens) = self.load_tokens() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());

                if let Some(exp) = tokens.expires_at {
                    if exp > now + 180 {
                        let mut lock = self.cached_tokens.write().await;
                        *lock = Some(tokens.clone());
                        return Ok(tokens.access_token);
                    }
                }

                // If expired or expiring soon, attempt refresh
                if let Some(ref refresh_tok) = tokens.refresh_token {
                    match self.refresh_token(refresh_tok).await {
                        Ok(new_tokens) => {
                            self.save_tokens(&new_tokens)?;
                            let mut lock = self.cached_tokens.write().await;
                            *lock = Some(new_tokens.clone());
                            return Ok(new_tokens.access_token);
                        }
                        Err(e) => {
                            eprintln!("Token refresh failed: {e}. Falling back to device login...");
                        }
                    }
                }
            }
        }

        // Perform Device Code Login
        let tokens = self.device_login().await?;
        self.save_tokens(&tokens)?;
        let mut lock = self.cached_tokens.write().await;
        *lock = Some(tokens.clone());
        Ok(tokens.access_token)
    }

    /// Refresh token during an active session if needed
    pub async fn refresh_current_token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let refresh_tok = {
            let lock = self.cached_tokens.read().await;
            lock.as_ref()
                .and_then(|t| t.refresh_token.clone())
                .or_else(|| self.load_tokens().and_then(|t| t.refresh_token))
        };

        if let Some(tok) = refresh_tok {
            let new_tokens = self.refresh_token(&tok).await?;
            self.save_tokens(&new_tokens)?;
            let mut lock = self.cached_tokens.write().await;
            let access = new_tokens.access_token.clone();
            *lock = Some(new_tokens);
            crate::logger::log_info("Successfully refreshed access token mid-session.");
            Ok(access)
        } else {
            Err("No refresh token available to refresh session.".into())
        }
    }

    /// Initiate `OAuth2` Device Code Flow
    async fn device_login(&self) -> Result<TokenStore, Box<dyn std::error::Error + Send + Sync>> {
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
            return Err(format!("Device code request failed: {err_text}").into());
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
        let deadline = Instant::now() + Duration::from_secs(device_code_data.expires_in);

        loop {
            if Instant::now() >= deadline {
                return Err("Device code expired before sign-in completed. Please rerun teams-tui.".into());
            }

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
                        return Err(format!("Login failed: {other} - {desc}").into());
                    }
                }
            }

            if let Some(access_tok) = body.access_token {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
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
    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenStore, Box<dyn std::error::Error + Send + Sync>> {
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
            return Err(format!("Token refresh request failed: {err_text}").into());
        }

        let body: TokenResponse = resp.json().await?;

        if let Some(access_tok) = body.access_token {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
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

    /// Load token from OS Keyring or explicitly permitted fallback file
    fn load_tokens(&self) -> Option<TokenStore> {
        // 1. Try OS Keyring
        match Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            Ok(entry) => match entry.get_password() {
                Ok(json_str) => match serde_json::from_str::<TokenStore>(&json_str) {
                    Ok(tokens) => return Some(tokens),
                    Err(e) => crate::logger::log_warn(format!("Failed to parse keyring tokens: {e}")),
                },
                Err(keyring::Error::NoEntry) => {}
                Err(e) => crate::logger::log_warn(format!("Keyring lookup error: {e}")),
            },
            Err(e) => crate::logger::log_warn(format!("Could not access OS Keyring entry: {e}")),
        }

        // 2. Fallback: local session file only if permitted
        if self.allow_insecure_token_file {
            let fallback_path = Self::fallback_token_path();
            if fallback_path.exists() {
                if let Ok(content) = fs::read_to_string(&fallback_path) {
                    if let Ok(tokens) = serde_json::from_str::<TokenStore>(&content) {
                        crate::logger::log_warn(format!(
                            "Loaded tokens from insecure fallback session file at {}",
                            fallback_path.display()
                        ));
                        return Some(tokens);
                    }
                }
            }
        }

        None
    }

    /// Save token to OS Keyring, falling back to local file ONLY if explicitly opted in
    fn save_tokens(&self, tokens: &TokenStore) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let json_str = serde_json::to_string(tokens)?;

        // Try OS Keyring first
        let keyring_err = match Entry::new(KEYRING_SERVICE, KEYRING_USER) {
            Ok(entry) => match entry.set_password(&json_str) {
                Ok(()) => {
                    // If an old fallback file exists from previous runs, remove it now that keyring succeeded
                    let fallback_path = Self::fallback_token_path();
                    if fallback_path.exists() {
                        let _ = fs::remove_file(fallback_path);
                    }
                    return Ok(());
                }
                Err(e) => Some(e.to_string()),
            },
            Err(e) => Some(e.to_string()),
        };

        let err_desc = keyring_err.unwrap_or_else(|| "Unknown keyring failure".to_string());

        if self.allow_insecure_token_file {
            crate::logger::log_warn(format!(
                "OS Keyring unavailable ({err_desc}). Storing tokens in fallback file because --allow-insecure-token-file is enabled."
            ));

            let fallback_path = Self::fallback_token_path();
            if let Some(parent) = fallback_path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            #[cfg(unix)]
            {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let mut f = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&fallback_path)?;
                f.write_all(json_str.as_bytes())?;
            }
            #[cfg(not(unix))]
            {
                fs::write(&fallback_path, json_str)?;
            }

            Ok(())
        } else {
            let msg = format!(
                "Secure OS Keyring storage failed ({err_desc}).\n\
                 Plaintext token fallback is blocked for security.\n\
                 To permit plaintext session storage in headless or unsupported environments, run with:\n\
                 teams-tui --allow-insecure-token-file"
            );
            crate::logger::log_error(&msg);
            Err(msg.into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_id_validation() {
        assert!(AuthManager::validate_tenant_id("common").is_ok());
        assert!(AuthManager::validate_tenant_id("organizations").is_ok());
        assert!(AuthManager::validate_tenant_id("my-tenant.onmicrosoft.com").is_ok());
        assert!(AuthManager::validate_tenant_id("11111111-2222-3333-4444-555555555555").is_ok());

        assert!(AuthManager::validate_tenant_id("").is_err());
        assert!(AuthManager::validate_tenant_id("tenant/with/slashes").is_err());
        assert!(AuthManager::validate_tenant_id("tenant?query=evil").is_err());
    }
}
