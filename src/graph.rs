use crate::auth::AuthManager;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct GraphList<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct UserProfile {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
    pub mail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrgUser {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "userPrincipalName")]
    pub user_principal_name: Option<String>,
    pub mail: Option<String>,
    #[serde(rename = "jobTitle")]
    pub job_title: Option<String>,
    pub department: Option<String>,
}

impl OrgUser {
    #[must_use]
    pub fn sanitized_display_name(&self) -> String {
        self.display_name
            .as_deref()
            .map_or_else(|| "Unknown".to_string(), strip_ansi_and_control)
    }

    #[must_use]
    pub fn sanitized_job_title(&self) -> String {
        self.job_title
            .as_deref()
            .map(strip_ansi_and_control)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn sanitized_department(&self) -> String {
        self.department
            .as_deref()
            .map(strip_ansi_and_control)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPersonEmail {
    address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GraphPerson {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "scoredEmailAddresses", default)]
    pub scored_email_addresses: Vec<GraphPersonEmail>,
    #[serde(rename = "jobTitle")]
    pub job_title: Option<String>,
    pub department: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresenceInfo {
    pub availability: String,
    pub activity: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMember {
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ChatSummary {
    pub id: String,
    pub topic: Option<String>,
    #[serde(rename = "chatType")]
    pub chat_type: String,
    #[serde(default)]
    pub members: Vec<ChatMember>,
    #[serde(rename = "lastUpdatedDateTime")]
    pub last_updated_date_time: Option<String>,
}

impl ChatSummary {
    #[must_use]
    pub fn resolved_title(&self, current_user_name: Option<&str>) -> String {
        if let Some(ref t) = self.topic {
            let clean = strip_ansi_and_control(t);
            if !clean.trim().is_empty() {
                return clean;
            }
        }

        // If 1:1 chat or group without title, summarize from member names
        let other_names: Vec<String> = self
            .members
            .iter()
            .filter_map(|m| m.display_name.as_deref())
            .filter(|name| {
                if let Some(me) = current_user_name {
                    !name.eq_ignore_ascii_case(me)
                } else {
                    true
                }
            })
            .map(strip_ansi_and_control)
            .collect();

        if other_names.is_empty() {
            let prefix = self.id.chars().take(8).collect::<String>();
            format!("Chat ({prefix})")
        } else {
            other_names.join(", ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TeamSummary {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: Option<String>,
}

impl TeamSummary {
    #[must_use]
    pub fn sanitized_name(&self) -> String {
        strip_ansi_and_control(&self.display_name)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ChannelSummary {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: Option<String>,
}

impl ChannelSummary {
    #[must_use]
    pub fn sanitized_name(&self) -> String {
        strip_ansi_and_control(&self.display_name)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessageFrom {
    pub user: Option<IdentityUser>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct IdentityUser {
    pub id: Option<String>,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageBody {
    #[serde(rename = "contentType")]
    pub content_type: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessageReaction {
    #[serde(rename = "reactionType")]
    pub reaction_type: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ChatMessage {
    pub id: String,
    #[serde(rename = "createdDateTime")]
    pub created_at: Option<String>,
    pub from: Option<ChatMessageFrom>,
    pub body: MessageBody,
    #[serde(default)]
    pub reactions: Vec<ChatMessageReaction>,
}

impl ChatMessage {
    /// Convert HTML or text content into readable plain text for TUI
    #[must_use]
    pub fn clean_content(&self) -> String {
        let text = if self.body.content_type.eq_ignore_ascii_case("html") {
            strip_html_tags(&self.body.content)
        } else {
            self.body.content.clone()
        };
        strip_ansi_and_control(&text)
    }

    #[must_use]
    pub fn sender_display_name(&self) -> String {
        self.from
            .as_ref()
            .and_then(|f| f.user.as_ref())
            .and_then(|u| u.display_name.as_deref())
            .map_or_else(|| "Unknown".to_string(), strip_ansi_and_control)
    }
}

/// Helper to strip ANSI escape codes (CSI, OSC) and Unicode bidirectional overrides
#[must_use]
pub fn strip_ansi_and_control(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        // Escape sequence start
        if ch == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    // CSI sequence: consume until terminating letter / symbol
                    chars.next();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                    continue;
                } else if next == ']' {
                    // OSC sequence: consume until BEL (\x07) or ST (\x1b\)
                    chars.next();
                    let mut prev = ' ';
                    for c in chars.by_ref() {
                        if c == '\x07' || (prev == '\x1b' && c == '\\') {
                            break;
                        }
                        prev = c;
                    }
                    continue;
                }
            }
            continue;
        }

        if ch == '\u{9b}' {
            // Single-byte CSI
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() || c == '~' {
                    break;
                }
            }
            continue;
        }

        // Filter Unicode bidirectional overrides and isolate controls
        if matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}') {
            continue;
        }

        // Retain normal whitespace and printable characters
        if ch == '\n' || ch == '\r' || ch == '\t' || !ch.is_control() {
            result.push(ch);
        }
    }
    result
}

#[must_use]
pub fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// Validate path segment IDs to prevent path traversal or malformed URL structures
fn sanitize_id(id: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('?') {
        return Err(format!("Invalid identifier in request: '{trimmed}'").into());
    }
    Ok(trimmed.to_string())
}

pub struct GraphClient {
    client: Client,
    token: Arc<RwLock<String>>,
    auth_manager: Option<Arc<AuthManager>>,
    base_url: String,
}

#[derive(Deserialize)]
struct PresenceResult {
    id: String,
    availability: String,
    activity: String,
}

impl GraphClient {
    pub fn new(token: String, auth_manager: Option<Arc<AuthManager>>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap_or_default();

        Self {
            client,
            token: Arc::new(RwLock::new(token)),
            auth_manager,
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    async fn current_bearer_token(&self) -> String {
        let lock = self.token.read().await;
        format!("Bearer {}", *lock)
    }

    /// Execute request with automatic 429 backoff and 401 token refresh retry
    async fn execute_with_retry(&self, make_request: impl Fn(&Client, &str) -> RequestBuilder) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let mut attempts = 0;
        let mut refreshed_token = false;

        loop {
            let auth_header = self.current_bearer_token().await;
            let req = make_request(&self.client, &auth_header);
            let start_time = std::time::Instant::now();
            let resp = req.send().await?;
            let status = resp.status();
            let elapsed_ms = start_time.elapsed().as_millis();

            if status == StatusCode::UNAUTHORIZED && !refreshed_token {
                if let Some(ref auth) = self.auth_manager {
                    crate::logger::log_warn("Graph API returned 401 Unauthorized. Attempting token refresh...");
                    if let Ok(new_tok) = auth.refresh_current_token().await {
                        let mut lock = self.token.write().await;
                        *lock = new_tok;
                        refreshed_token = true;
                        continue;
                    }
                }
            }

            if status == StatusCode::TOO_MANY_REQUESTS && attempts < 3 {
                attempts += 1;
                let retry_after_secs = resp
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2 * attempts);

                crate::logger::log_warn(format!(
                    "Graph API 429 rate limit hit. Waiting {retry_after_secs}s (attempt {attempts})..."
                ));

                sleep(Duration::from_secs(retry_after_secs)).await;
                continue;
            }

            crate::logger::log_debug(format!(
                "Graph API HTTP {status} (roundtrip: {elapsed_ms}ms)"
            ));

            return Ok(resp);
        }
    }

    async fn send_and_parse<T: for<'de> Deserialize<'de>>(&self, make_request: impl Fn(&Client, &str) -> RequestBuilder) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
        let resp = self.execute_with_retry(make_request).await?;
        let status = resp.status();
        if !status.is_success() {
            let err_txt = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Graph API error HTTP {status}: {err_txt}").into());
        }
        let data = resp.json::<T>().await?;
        Ok(data)
    }

    /// Fetch signed-in user's profile
    pub async fn get_me(&self) -> Result<UserProfile, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me", self.base_url);
        self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await
    }

    /// Fetch user's Teams presence (e.g. Available, Busy, Away)
    pub async fn get_presence(&self) -> Result<PresenceInfo, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/presence", self.base_url);
        self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await
    }

    /// Set user's preferred presence
    pub async fn set_preferred_presence(
        &self,
        user_id: &str,
        availability: &str,
        activity: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let valid_uid = sanitize_id(user_id)?;
        let url = format!("{}/users/{valid_uid}/presence/setUserPreferredPresence", self.base_url);
        let body = serde_json::json!({
            "availability": availability,
            "activity": activity,
            "expirationDuration": "PT8H"
        });

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to set presence: {err_txt}").into());
        }
        Ok(())
    }

    /// Establish an application-level presence session so the user doesn't appear Offline
    pub async fn establish_presence_session(&self, client_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/presence/setPresence", self.base_url);
        let body = serde_json::json!({
            "sessionId": client_id,
            "availability": "Available",
            "activity": "Available",
            "expirationDuration": "PT1H"
        });

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to establish presence session: {err_txt}").into());
        }
        Ok(())
    }

    /// Clear user's preferred presence (revert to system default)
    pub async fn clear_preferred_presence(
        &self,
        user_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let valid_uid = sanitize_id(user_id)?;
        let url = format!("{}/users/{valid_uid}/presence/clearUserPreferredPresence", self.base_url);

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to clear presence: {err_txt}").into());
        }
        Ok(())
    }

    /// Fetch presence for multiple users
    pub async fn get_users_presence(
        &self,
        user_ids: &[&str],
    ) -> Result<Vec<(String, PresenceInfo)>, Box<dyn std::error::Error + Send + Sync>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/communications/getPresencesByUserId", self.base_url);
        let body = serde_json::json!({
            "ids": user_ids
        });

        let data: GraphList<PresenceResult> = self.send_and_parse(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        let result = data.value.into_iter().map(|p| {
            (
                p.id,
                PresenceInfo {
                    availability: p.availability,
                    activity: p.activity,
                }
            )
        }).collect();

        Ok(result)
    }

    /// List 1:1 and Group chats
    pub async fn get_my_chats(&self) -> Result<Vec<ChatSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/chats?$expand=members&$top=30", self.base_url);
        let data: GraphList<ChatSummary> = self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await?;
        Ok(data.value)
    }

    /// List Teams user has joined
    pub async fn get_joined_teams(&self) -> Result<Vec<TeamSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/joinedTeams", self.base_url);
        let data: GraphList<TeamSummary> = self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await?;
        Ok(data.value)
    }

    /// List Channels within a Team
    pub async fn get_team_channels(
        &self,
        team_id: &str,
    ) -> Result<Vec<ChannelSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let valid_tid = sanitize_id(team_id)?;
        let url = format!("{}/teams/{valid_tid}/channels", self.base_url);
        let data: GraphList<ChannelSummary> = self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await?;
        Ok(data.value)
    }

    /// Fetch latest messages from a Chat
    pub async fn get_chat_messages(
        &self,
        chat_id: &str,
    ) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error + Send + Sync>> {
        let valid_cid = sanitize_id(chat_id)?;
        let url = format!("{}/me/chats/{valid_cid}/messages?$top=30", self.base_url);
        let data: GraphList<ChatMessage> = self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await?;
        Ok(data.value)
    }

    /// Fetch latest messages from a Team Channel (top=20 for optimal latency)
    pub async fn get_channel_messages(
        &self,
        team_id: &str,
        channel_id: &str,
    ) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error + Send + Sync>> {
        let team_id_clean = sanitize_id(team_id)?;
        let channel_id_clean = sanitize_id(channel_id)?;
        let url = format!("{}/teams/{team_id_clean}/channels/{channel_id_clean}/messages?$top=20", self.base_url);
        let data: GraphList<ChatMessage> = self.send_and_parse(|client, auth| {
            client.get(&url).header("Authorization", auth)
        }).await?;
        Ok(data.value)
    }

    /// Send a text message to a 1:1 or Group Chat
    pub async fn send_chat_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let valid_cid = sanitize_id(chat_id)?;
        let url = format!("{}/me/chats/{valid_cid}/messages", self.base_url);
        let body = serde_json::json!({
            "body": {
                "contentType": "text",
                "content": text
            }
        });

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to send chat message: {err_txt}").into());
        }
        Ok(())
    }

    /// Send a text message to a Team Channel
    pub async fn send_channel_message(
        &self,
        team_id: &str,
        channel_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let team_id_clean = sanitize_id(team_id)?;
        let channel_id_clean = sanitize_id(channel_id)?;
        let url = format!("{}/teams/{team_id_clean}/channels/{channel_id_clean}/messages", self.base_url);
        let body = serde_json::json!({
            "body": {
                "contentType": "text",
                "content": text
            }
        });

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to send channel message: {err_txt}").into());
        }
        Ok(())
    }

    /// Search for users in the organisation by display name or email, falling back to /me/people if directory access is restricted
    pub async fn search_users(
        &self,
        query: &str,
    ) -> Result<Vec<OrgUser>, Box<dyn std::error::Error + Send + Sync>> {
        let clean_q = query.trim().replace('\'', "''");
        if clean_q.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/users", self.base_url);
        let filter = format!(
            "startswith(displayName,'{clean_q}') or startswith(userPrincipalName,'{clean_q}') or startswith(mail,'{clean_q}')"
        );

        let resp = self.execute_with_retry(|client, auth| {
            client.get(&url)
                .header("Authorization", auth)
                .query(&[
                    ("$filter", filter.as_str()),
                    ("$top", "15"),
                    ("$select", "id,displayName,userPrincipalName,mail,jobTitle,department"),
                ])
        }).await?;

        if resp.status() == StatusCode::FORBIDDEN {
            crate::logger::log_warn(
                "/users returned 403 Forbidden. Falling back to /me/people endpoint..."
            );
            let people_url = format!("{}/me/people", self.base_url);
            let escaped_search = clean_q.replace('\\', "\\\\").replace('"', "\\\"");
            let search_val = format!("\"{escaped_search}\"");

            if let Ok(people_resp) = self.execute_with_retry(|client, auth| {
                client.get(&people_url)
                    .header("Authorization", auth)
                    .query(&[
                        ("$search", search_val.as_str()),
                        ("$top", "15"),
                    ])
            }).await {
                if people_resp.status().is_success() {
                    if let Ok(people_data) = people_resp.json::<GraphList<GraphPerson>>().await {
                        let users = people_data
                            .value
                            .into_iter()
                            .map(|p| {
                                let email = p
                                    .scored_email_addresses
                                    .into_iter()
                                    .next()
                                    .and_then(|e| e.address);
                                OrgUser {
                                    id: p.id,
                                    display_name: p.display_name,
                                    user_principal_name: email.clone(),
                                    mail: email,
                                    job_title: p.job_title,
                                    department: p.department,
                                }
                            })
                            .collect();
                        return Ok(users);
                    }
                }
            }

            return Err("PERMISSION_DENIED: Directory search requires 'User.ReadBasic.All'. Please run 'teams-tui --login' to consent to updated permissions.".into());
        }

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("User search failed: {err_txt}").into());
        }

        let data = resp.json::<GraphList<OrgUser>>().await?;
        Ok(data.value)
    }

    /// Find or create a 1:1 chat with a target user in the organisation
    pub async fn get_or_create_1on1_chat(
        &self,
        my_id: &str,
        target_user_id: &str,
    ) -> Result<ChatSummary, Box<dyn std::error::Error + Send + Sync>> {
        let valid_my_id = sanitize_id(my_id)?;
        let valid_target_id = sanitize_id(target_user_id)?;
        let url = format!("{}/chats", self.base_url);
        let body = serde_json::json!({
            "chatType": "oneOnOne",
            "members": [
                {
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": format!("{}/users('{valid_my_id}')", self.base_url)
                },
                {
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": format!("{}/users('{valid_target_id}')", self.base_url)
                }
            ]
        });

        let resp = self.execute_with_retry(|client, auth| {
            client.post(&url).header("Authorization", auth).json(&body)
        }).await?;

        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to open/create chat: {err_txt}").into());
        }

        let chat = resp.json::<ChatSummary>().await?;
        Ok(chat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_and_control() {
        let text_with_csi = "\x1b[31;1mRed Alert!\x1b[0m";
        assert_eq!(strip_ansi_and_control(text_with_csi), "Red Alert!");

        let text_with_osc = "\x1b]0;Evil Title\x07Hello World";
        assert_eq!(strip_ansi_and_control(text_with_osc), "Hello World");

        let text_with_bidi = "Admin\u{202E}txt.exe";
        assert_eq!(strip_ansi_and_control(text_with_bidi), "Admintxt.exe");
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello &amp; <b>welcome</b> to &quot;Teams&quot;!</p>";
        assert_eq!(strip_html_tags(html), "Hello & welcome to \"Teams\"!");
    }

    #[test]
    fn test_sanitize_id() {
        assert!(sanitize_id("valid-id-123").is_ok());
        assert!(sanitize_id("../bad/path").is_err());
        assert!(sanitize_id("evil?query=1").is_err());
        assert!(sanitize_id("").is_err());
    }
}
