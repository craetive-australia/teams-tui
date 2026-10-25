use crate::graph::{ChannelSummary, ChatMessage, ChatSummary, OrgUser, PresenceInfo, TeamSummary, UserProfile};
use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FocusedPane {
    Sidebar,
    Messages,
    Input,
}

#[derive(Debug, Clone)]
pub enum SidebarItem {
    SectionHeader(String),
    Chat {
        chat: ChatSummary,
        title: String,
    },
    TeamHeader {
        team: TeamSummary,
        #[allow(dead_code)]
        is_expanded: bool,
    },
    Channel {
        team_id: String,
        team_name: String,
        channel: ChannelSummary,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveConversation {
    Chat {
        id: String,
        title: String,
    },
    Channel {
        team_id: String,
        channel_id: String,
        title: String,
    },
}

impl ActiveConversation {
    #[must_use]
    pub fn cache_key(&self) -> String {
        match self {
            ActiveConversation::Chat { id, .. } => format!("chat:{id}"),
            ActiveConversation::Channel { team_id, channel_id, .. } => {
                format!("channel:{team_id}:{channel_id}")
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct SearchModalState {
    pub is_open: bool,
    pub query: String,
    pub results: Vec<OrgUser>,
    pub selected_idx: usize,
    pub is_searching: bool,
    pub directory_search_denied: bool,
    pub error_msg: Option<String>,
}

#[derive(Default, Debug)]
pub struct StatusModalState {
    pub is_open: bool,
    pub selected_idx: usize,
}

pub struct App {
    pub current_user: Option<UserProfile>,
    pub presence: Option<PresenceInfo>,
    pub sidebar_items: Vec<SidebarItem>,
    pub selected_sidebar_idx: usize,
    pub active_conversation: Option<ActiveConversation>,
    pub messages: Vec<ChatMessage>,
    pub message_scroll: usize,
    pub input_buffer: String,
    /// Character index (not raw byte offset)
    pub cursor_pos: usize,
    pub focused_pane: FocusedPane,
    pub status_message: String,
    pub is_loading: bool,
    pub is_syncing: bool,
    pub should_quit: bool,
    // Conversation cache
    pub message_cache: HashMap<String, Vec<ChatMessage>>,
    pub user_presences: HashMap<String, PresenceInfo>,
    // Raw lists for fast contact lookups & channel dynamic updates
    pub raw_chats: Vec<ChatSummary>,
    pub raw_teams: Vec<(TeamSummary, Vec<ChannelSummary>)>,
    // Modals
    pub search: SearchModalState,
    pub status_modal: StatusModalState,
}

pub const STATUS_OPTIONS: &[(&str, &str, &str, &str)] = &[
    ("Available", "🟢", "Available", "Available"),
    ("Busy", "🔴", "Busy", "Busy"),
    ("Do Not Disturb", "⛔", "DoNotDisturb", "DoNotDisturb"),
    ("Be Right Back", "🟡", "BeRightBack", "BeRightBack"),
    ("Away", "🟡", "Away", "Away"),
    ("Reset (Auto)", "🔄", "", ""),
];

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_user: None,
            presence: None,
            sidebar_items: Vec::new(),
            selected_sidebar_idx: 0,
            active_conversation: None,
            messages: Vec::new(),
            message_scroll: 0,
            input_buffer: String::new(),
            cursor_pos: 0,
            focused_pane: FocusedPane::Sidebar,
            status_message: "Ready".to_string(),
            is_loading: false,
            is_syncing: false,
            should_quit: false,
            message_cache: HashMap::new(),
            user_presences: HashMap::new(),
            raw_chats: Vec::new(),
            raw_teams: Vec::new(),
            search: SearchModalState::default(),
            status_modal: StatusModalState::default(),
        }
    }

    pub fn open_search(&mut self) {
        self.search.is_open = true;
        self.search.query.clear();
        self.search.results.clear();
        self.search.selected_idx = 0;
        self.search.is_searching = false;
        self.search.error_msg = None;
    }

    pub fn close_search(&mut self) {
        self.search.is_open = false;
        self.search.query.clear();
        self.search.results.clear();
        self.search.selected_idx = 0;
        self.search.is_searching = false;
        self.search.error_msg = None;
    }

    pub fn open_status_modal(&mut self) {
        self.status_modal.is_open = true;
        self.status_modal.selected_idx = 0;
    }

    pub fn close_status_modal(&mut self) {
        self.status_modal.is_open = false;
    }

    pub fn status_up(&mut self) {
        if self.status_modal.selected_idx == 0 {
            self.status_modal.selected_idx = STATUS_OPTIONS.len() - 1;
        } else {
            self.status_modal.selected_idx -= 1;
        }
    }

    pub fn status_down(&mut self) {
        self.status_modal.selected_idx = (self.status_modal.selected_idx + 1) % STATUS_OPTIONS.len();
    }

    pub fn search_char(&mut self, c: char) {
        self.search.query.push(c);
    }

    pub fn search_backspace(&mut self) {
        self.search.query.pop();
    }

    pub fn search_down(&mut self) {
        if !self.search.results.is_empty() {
            self.search.selected_idx = (self.search.selected_idx + 1) % self.search.results.len();
        }
    }

    pub fn search_up(&mut self) {
        if !self.search.results.is_empty() {
            if self.search.selected_idx == 0 {
                self.search.selected_idx = self.search.results.len() - 1;
            } else {
                self.search.selected_idx -= 1;
            }
        }
    }

    #[must_use]
    pub fn selected_search_user(&self) -> Option<&OrgUser> {
        self.search.results.get(self.search.selected_idx)
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = msg.into();
    }

    pub fn cycle_focus(&mut self) {
        self.focused_pane = match self.focused_pane {
            FocusedPane::Sidebar => FocusedPane::Messages,
            FocusedPane::Messages => FocusedPane::Input,
            FocusedPane::Input => FocusedPane::Sidebar,
        };
    }

    pub fn sidebar_down(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }

        let mut next = self.selected_sidebar_idx;
        for _ in 0..self.sidebar_items.len() {
            next = (next + 1) % self.sidebar_items.len();
            if !matches!(self.sidebar_items[next], SidebarItem::SectionHeader(_)) {
                self.selected_sidebar_idx = next;
                break;
            }
        }
    }

    pub fn sidebar_up(&mut self) {
        if self.sidebar_items.is_empty() {
            return;
        }

        let mut prev = self.selected_sidebar_idx;
        for _ in 0..self.sidebar_items.len() {
            if prev == 0 {
                prev = self.sidebar_items.len() - 1;
            } else {
                prev -= 1;
            }
            if !matches!(self.sidebar_items[prev], SidebarItem::SectionHeader(_)) {
                self.selected_sidebar_idx = prev;
                break;
            }
        }
    }

    pub fn scroll_messages_up(&mut self) {
        self.message_scroll = self.message_scroll.saturating_add(3);
    }

    pub fn scroll_messages_down(&mut self) {
        self.message_scroll = self.message_scroll.saturating_sub(3);
    }

    /// UTF-8 safe char insertion at current character index
    pub fn insert_char(&mut self, c: char) {
        let byte_offset = self
            .input_buffer
            .char_indices()
            .nth(self.cursor_pos)
            .map_or(self.input_buffer.len(), |(idx, _)| idx);

        self.input_buffer.insert(byte_offset, c);
        self.cursor_pos += 1;
    }

    /// UTF-8 safe char removal before current character index
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 && !self.input_buffer.is_empty() {
            self.cursor_pos -= 1;
            let byte_offset = self
                .input_buffer
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(idx, _)| idx);

            if let Some(idx) = byte_offset {
                self.input_buffer.remove(idx);
            }
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let char_count = self.input_buffer.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    pub fn clear_input(&mut self) -> String {
        let text = self.input_buffer.clone();
        self.input_buffer.clear();
        self.cursor_pos = 0;
        text
    }

    #[must_use]
    pub fn currently_selected_conversation(&self) -> Option<ActiveConversation> {
        match self.sidebar_items.get(self.selected_sidebar_idx)? {
            SidebarItem::Chat { chat, title } => Some(ActiveConversation::Chat {
                id: chat.id.clone(),
                title: title.clone(),
            }),
            SidebarItem::Channel {
                team_id,
                channel,
                team_name,
            } => Some(ActiveConversation::Channel {
                team_id: team_id.clone(),
                channel_id: channel.id.clone(),
                title: format!("{team_name} > #{}", channel.sanitized_name()),
            }),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_cached_messages(&self, conv: &ActiveConversation) -> Option<&Vec<ChatMessage>> {
        self.message_cache.get(&conv.cache_key())
    }

    pub fn cache_messages(&mut self, conv: &ActiveConversation, msgs: Vec<ChatMessage>) {
        self.message_cache.insert(conv.cache_key(), msgs);
    }

    /// Search local contacts from existing chats and teams instantly (0ms latency, works offline / without directory permissions)
    #[must_use]
    pub fn search_local_contacts(&self, query: &str) -> Vec<OrgUser> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }

        let mut seen = HashSet::new();
        let mut results = Vec::new();

        let me_name = self
            .current_user
            .as_ref()
            .and_then(|u| u.display_name.as_deref())
            .unwrap_or("")
            .to_lowercase();

        for chat in &self.raw_chats {
            for member in &chat.members {
                if let Some(ref name) = member.display_name {
                    if !name.trim().is_empty() && name.to_lowercase() != me_name {
                        let email = member.email.as_deref().unwrap_or("");
                        if (name.to_lowercase().contains(&q) || email.to_lowercase().contains(&q))
                            && seen.insert(name.to_lowercase())
                        {
                            results.push(OrgUser {
                                id: member.user_id.clone().or_else(|| member.id.clone()).unwrap_or_default(),
                                display_name: Some(name.clone()),
                                user_principal_name: member.email.clone(),
                                mail: member.email.clone(),
                                job_title: Some("Recent Chat Contact".to_string()),
                                department: None,
                            });
                        }
                    }
                }
            }
        }

        results
    }

    /// Find an existing 1:1 chat for a given user if already in chat list
    #[must_use]
    pub fn find_existing_chat(&self, target_name: &str, target_id: &str) -> Option<ChatSummary> {
        for chat in &self.raw_chats {
            if !target_id.is_empty()
                && chat.members.iter().any(|m| {
                    m.user_id.as_deref() == Some(target_id) || m.id.as_deref() == Some(target_id)
                })
            {
                return Some(chat.clone());
            }
            if chat.members.iter().any(|m| {
                m.display_name
                    .as_deref()
                    .unwrap_or("")
                    .eq_ignore_ascii_case(target_name)
            }) {
                return Some(chat.clone());
            }
        }
        None
    }

    /// Dynamically update channels for a team loaded asynchronously in the background
    pub fn update_team_channels(&mut self, team_id: &str, channels: Vec<ChannelSummary>) {
        for (team, ch_list) in &mut self.raw_teams {
            if team.id == team_id {
                *ch_list = channels;
                break;
            }
        }
        self.rebuild_sidebar_items();
    }

    pub fn rebuild_sidebar(
        &mut self,
        chats: &[ChatSummary],
        teams: &[(TeamSummary, Vec<ChannelSummary>)],
    ) {
        self.raw_chats = chats.to_vec();
        self.raw_teams = teams.to_vec();
        self.rebuild_sidebar_items();
    }

    fn rebuild_sidebar_items(&mut self) {
        let me_name = self
            .current_user
            .as_ref()
            .and_then(|u| u.display_name.clone());

        let mut items = Vec::new();

        // Chats section
        if !self.raw_chats.is_empty() {
            items.push(SidebarItem::SectionHeader("── CHATS ──".to_string()));
            for chat in &self.raw_chats {
                let title = chat.resolved_title(me_name.as_deref());
                items.push(SidebarItem::Chat {
                    chat: chat.clone(),
                    title,
                });
            }
        }

        // Teams & Channels section
        if !self.raw_teams.is_empty() {
            items.push(SidebarItem::SectionHeader("── TEAMS ──".to_string()));
            for (team, channels) in &self.raw_teams {
                items.push(SidebarItem::TeamHeader {
                    team: team.clone(),
                    is_expanded: true,
                });
                for ch in channels {
                    items.push(SidebarItem::Channel {
                        team_id: team.id.clone(),
                        team_name: team.sanitized_name(),
                        channel: ch.clone(),
                    });
                }
            }
        }

        self.sidebar_items = items;

        // Position on first selectable item
        if self.selected_sidebar_idx >= self.sidebar_items.len()
            || matches!(
                self.sidebar_items.get(self.selected_sidebar_idx),
                Some(SidebarItem::SectionHeader(_))
            )
        {
            self.selected_sidebar_idx = self
                .sidebar_items
                .iter()
                .position(|item| !matches!(item, SidebarItem::SectionHeader(_)))
                .unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_input_and_backspace() {
        let mut app = App::new();

        // Insert multi-byte emoji and accents
        app.insert_char('H');
        app.insert_char('é');
        app.insert_char('l');
        app.insert_char('l');
        app.insert_char('o');
        app.insert_char(' ');
        app.insert_char('👋');

        assert_eq!(app.input_buffer, "Héllo 👋");
        assert_eq!(app.cursor_pos, 7);

        // Move left across multi-byte boundary
        app.move_cursor_left();
        assert_eq!(app.cursor_pos, 6);

        // Insert char before emoji
        app.insert_char('!');
        assert_eq!(app.input_buffer, "Héllo !👋");
        assert_eq!(app.cursor_pos, 7);

        // Backspace removes the '!'
        app.backspace();
        assert_eq!(app.input_buffer, "Héllo 👋");
        assert_eq!(app.cursor_pos, 6);

        // Move to end and backspace the emoji
        app.move_cursor_right();
        assert_eq!(app.cursor_pos, 7);
        app.backspace();
        assert_eq!(app.input_buffer, "Héllo ");
        assert_eq!(app.cursor_pos, 6);
    }

    #[test]
    fn test_active_conversation_cache_key() {
        let chat = ActiveConversation::Chat {
            id: "123".to_string(),
            title: "Test".to_string(),
        };
        assert_eq!(chat.cache_key(), "chat:123");

        let channel = ActiveConversation::Channel {
            team_id: "teamA".to_string(),
            channel_id: "general".to_string(),
            title: "General".to_string(),
        };
        assert_eq!(channel.cache_key(), "channel:teamA:general");
    }

    #[test]
    fn test_search_local_contacts() {
        let mut app = App::new();
        app.current_user = Some(UserProfile {
            id: "me-id".to_string(),
            display_name: Some("Myself".to_string()),
            user_principal_name: Some("me@example.com".to_string()),
            mail: Some("me@example.com".to_string()),
        });

        app.raw_chats = vec![ChatSummary {
            id: "chat-1".to_string(),
            topic: None,
            chat_type: "oneOnOne".to_string(),
            members: vec![
                crate::graph::ChatMember {
                    id: Some("m1".to_string()),
                    display_name: Some("Alice Smith".to_string()),
                    user_id: Some("alice-id".to_string()),
                    email: Some("alice@example.com".to_string()),
                },
                crate::graph::ChatMember {
                    id: Some("m2".to_string()),
                    display_name: Some("Myself".to_string()),
                    user_id: Some("me-id".to_string()),
                    email: Some("me@example.com".to_string()),
                },
            ],
            last_updated_date_time: None,
        }];

        let results = app.search_local_contacts("alice");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].display_name.as_deref(), Some("Alice Smith"));

        let found = app.find_existing_chat("Alice Smith", "alice-id");
        assert!(found.is_some());
    }
}
