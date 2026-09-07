use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
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
#[allow(dead_code)]
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
    pub fn resolved_title(&self, current_user_name: Option<&str>) -> String {
        if let Some(ref t) = self.topic {
            if !t.trim().is_empty() {
                return t.clone();
            }
        }

        // If 1:1 chat or group without title, summarize from member names
        let other_names: Vec<String> = self
            .members
            .iter()
            .filter_map(|m| m.display_name.clone())
            .filter(|name| {
                if let Some(me) = current_user_name {
                    name.to_lowercase() != me.to_lowercase()
                } else {
                    true
                }
            })
            .collect();

        if !other_names.is_empty() {
            other_names.join(", ")
        } else {
            format!("Chat ({})", &self.id[..self.id.len().min(8)])
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

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ChannelSummary {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: Option<String>,
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
    pub fn clean_content(&self) -> String {
        if self.body.content_type.eq_ignore_ascii_case("html") {
            strip_html_tags(&self.body.content)
        } else {
            self.body.content.clone()
        }
    }

    pub fn sender_display_name(&self) -> String {
        self.from
            .as_ref()
            .and_then(|f| f.user.as_ref())
            .and_then(|u| u.display_name.clone())
            .unwrap_or_else(|| "Unknown".to_string())
    }
}

/// Helper to strip basic HTML tags and replace common entities
fn strip_html_tags(html: &str) -> String {
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

pub struct GraphClient {
    client: Client,
    base_url: String,
}

impl GraphClient {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        let auth_val = format!("Bearer {}", token);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_val).expect("Invalid auth header value"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url: "https://graph.microsoft.com/v1.0".to_string(),
        }
    }

    /// Execute request with automatic 429 (Too Many Requests) backoff
    async fn execute_with_retry(&self, req_builder: reqwest::RequestBuilder) -> Result<Response, reqwest::Error> {
        let mut attempts = 0;
        let start_time = std::time::Instant::now();
        loop {
            let req = req_builder
                .try_clone()
                .expect("Request builder must be cloneable for retries");
            let resp = req.send().await?;
            let status = resp.status();
            let elapsed_ms = start_time.elapsed().as_millis();

            if status == StatusCode::TOO_MANY_REQUESTS && attempts < 3 {
                attempts += 1;
                let retry_after_secs = resp
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2 * attempts);

                crate::logger::log_warn(format!(
                    "Graph API 429 rate limit hit. Waiting {}s (attempt {})...",
                    retry_after_secs, attempts
                ));

                sleep(Duration::from_secs(retry_after_secs)).await;
                continue;
            }

            crate::logger::log_debug(format!(
                "Graph API HTTP {} (roundtrip: {}ms)",
                status, elapsed_ms
            ));

            return Ok(resp);
        }
    }

    /// Fetch signed-in user's profile
    pub async fn get_me(&self) -> Result<UserProfile, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me", self.base_url);
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let profile = resp.json::<UserProfile>().await?;
        Ok(profile)
    }

    /// Fetch user's Teams presence (e.g. Available, Busy, Away)
    pub async fn get_presence(&self) -> Result<PresenceInfo, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/presence", self.base_url);
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let presence = resp.json::<PresenceInfo>().await?;
        Ok(presence)
    }

    /// Set user's preferred presence
    pub async fn set_preferred_presence(
        &self,
        user_id: &str,
        availability: &str,
        activity: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/users/{}/presence/setUserPreferredPresence", self.base_url, user_id);
        let body = serde_json::json!({
            "availability": availability,
            "activity": activity,
            "expirationDuration": "PT8H"
        });
        
        let req = self.client.post(&url).json(&body);
        let resp = self.execute_with_retry(req).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to set presence: {}", err_txt).into());
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
        let req = self.client.post(&url).json(&body);
        let resp = self.execute_with_retry(req).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to establish presence session: {}", err_txt).into());
        }
        Ok(())
    }

    /// Clear user's preferred presence (revert to system default)
    pub async fn clear_preferred_presence(
        &self,
        user_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/users/{}/presence/clearUserPreferredPresence", self.base_url, user_id);
        let req = self.client.post(&url);
        let resp = self.execute_with_retry(req).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to clear presence: {}", err_txt).into());
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

        let req = self.client.post(&url).json(&body);
        let resp = self.execute_with_retry(req).await?;
        
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to fetch user presences: {}", err_txt).into());
        }

        #[derive(Deserialize)]
        struct PresenceResult {
            id: String,
            availability: String,
            activity: String,
        }

        let data = resp.json::<GraphList<PresenceResult>>().await?;
        
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
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let data = resp.json::<GraphList<ChatSummary>>().await?;
        Ok(data.value)
    }

    /// List Teams user has joined
    pub async fn get_joined_teams(&self) -> Result<Vec<TeamSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/joinedTeams", self.base_url);
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let data = resp.json::<GraphList<TeamSummary>>().await?;
        Ok(data.value)
    }

    /// List Channels within a Team
    pub async fn get_team_channels(
        &self,
        team_id: &str,
    ) -> Result<Vec<ChannelSummary>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/teams/{}/channels", self.base_url, team_id);
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let data = resp.json::<GraphList<ChannelSummary>>().await?;
        Ok(data.value)
    }

    /// Fetch latest messages from a Chat
    pub async fn get_chat_messages(
        &self,
        chat_id: &str,
    ) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/chats/{}/messages?$top=30", self.base_url, chat_id);
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let data = resp.json::<GraphList<ChatMessage>>().await?;
        Ok(data.value)
    }

    /// Fetch latest messages from a Team Channel (top=20 for optimal latency)
    pub async fn get_channel_messages(
        &self,
        team_id: &str,
        channel_id: &str,
    ) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/teams/{}/channels/{}/messages?$top=20",
            self.base_url, team_id, channel_id
        );
        let resp = self.execute_with_retry(self.client.get(&url)).await?;
        let data = resp.json::<GraphList<ChatMessage>>().await?;
        Ok(data.value)
    }

    /// Send a text message to a 1:1 or Group Chat
    pub async fn send_chat_message(
        &self,
        chat_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/me/chats/{}/messages", self.base_url, chat_id);
        let body = serde_json::json!({
            "body": {
                "contentType": "text",
                "content": text
            }
        });
        let resp = self.execute_with_retry(self.client.post(&url).json(&body)).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to send chat message: {}", err_txt).into());
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
        let url = format!(
            "{}/teams/{}/channels/{}/messages",
            self.base_url, team_id, channel_id
        );
        let body = serde_json::json!({
            "body": {
                "contentType": "text",
                "content": text
            }
        });
        let resp = self.execute_with_retry(self.client.post(&url).json(&body)).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to send channel message: {}", err_txt).into());
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
            "startswith(displayName,'{}') or startswith(userPrincipalName,'{}') or startswith(mail,'{}')",
            clean_q, clean_q, clean_q
        );

        let req = self.client.get(&url).query(&[
            ("$filter", filter.as_str()),
            ("$top", "15"),
            ("$select", "id,displayName,userPrincipalName,mail,jobTitle,department"),
        ]);

        let resp = self.execute_with_retry(req).await?;
        if resp.status() == StatusCode::FORBIDDEN {
            // Attempt fallback to /me/people endpoint (requires only People.Read)
            crate::logger::log_warn(
                "/users returned 403 Forbidden. Falling back to /me/people endpoint..."
            );
            let people_url = format!("{}/me/people", self.base_url);
            let people_req = self.client.get(&people_url).query(&[
                ("$search", format!("\"{}\"", clean_q).as_str()),
                ("$top", "15"),
            ]);
            if let Ok(people_resp) = self.execute_with_retry(people_req).await {
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
            return Err(format!("User search failed: {}", err_txt).into());
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
        let url = format!("{}/chats", self.base_url);
        let body = serde_json::json!({
            "chatType": "oneOnOne",
            "members": [
                {
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": format!("{}/users('{}')", self.base_url, my_id)
                },
                {
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": format!("{}/users('{}')", self.base_url, target_user_id)
                }
            ]
        });

        let resp = self.execute_with_retry(self.client.post(&url).json(&body)).await?;
        if !resp.status().is_success() {
            let err_txt = resp.text().await?;
            return Err(format!("Failed to open/create chat: {}", err_txt).into());
        }

        let chat = resp.json::<ChatSummary>().await?;
        Ok(chat)
    }
}
