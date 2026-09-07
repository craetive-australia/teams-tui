use crate::graph::{ChannelSummary, ChatMessage, ChatSummary, OrgUser, PresenceInfo, TeamSummary, UserProfile};

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
    pub fn cache_key(&self) -> String {
        match self {
            ActiveConversation::Chat { id, .. } => format!("chat:{}", id),
            ActiveConversation::Channel { team_id, channel_id, .. } => {
                format!("channel:{}:{}", team_id, channel_id)
            }
        }
    }
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
    pub cursor_pos: usize,
    pub focused_pane: FocusedPane,
    pub status_message: String,
    pub is_loading: bool,
    pub is_syncing: bool,
    pub should_quit: bool,
    // Conversation cache
    pub message_cache: std::collections::HashMap<String, Vec<ChatMessage>>,
    pub user_presences: std::collections::HashMap<String, PresenceInfo>,
    // Raw lists for fast contact lookups & channel dynamic updates
    pub raw_chats: Vec<ChatSummary>,
    pub raw_teams: Vec<(TeamSummary, Vec<ChannelSummary>)>,
    // Organisation User Search Modal
    pub is_search_modal_open: bool,
    pub search_query: String,
    pub search_results: Vec<OrgUser>,
    pub selected_search_idx: usize,
    pub is_searching: bool,
    pub directory_search_denied: bool,
    pub search_error_msg: Option<String>,
    // Status Modal
    pub is_status_modal_open: bool,
    pub selected_status_idx: usize,
}

pub const STATUS_OPTIONS: &[(&str, &str, &str, &str)] = &[
    ("Available", "🟢", "Available", "Available"),
    ("Busy", "🔴", "Busy", "Busy"),
    ("Do Not Disturb", "⛔", "DoNotDisturb", "DoNotDisturb"),
    ("Be Right Back", "🟡", "BeRightBack", "BeRightBack"),
    ("Away", "🟡", "Away", "Away"),
    ("Reset (Auto)", "🔄", "", ""),
];

impl App {
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
            message_cache: std::collections::HashMap::new(),
            user_presences: std::collections::HashMap::new(),
            raw_chats: Vec::new(),
            raw_teams: Vec::new(),
            is_search_modal_open: false,
            search_query: String::new(),
            search_results: Vec::new(),
            selected_search_idx: 0,
            is_searching: false,
            directory_search_denied: false,
            search_error_msg: None,
            is_status_modal_open: false,
            selected_status_idx: 0,
        }
    }

    pub fn open_search(&mut self) {
        self.is_search_modal_open = true;
        self.search_query.clear();
        self.search_results.clear();
        self.selected_search_idx = 0;
        self.is_searching = false;
        self.search_error_msg = None;
    }

    pub fn close_search(&mut self) {
        self.is_search_modal_open = false;
        self.search_query.clear();
        self.search_results.clear();
        self.selected_search_idx = 0;
        self.is_searching = false;
        self.search_error_msg = None;
    }

    pub fn open_status_modal(&mut self) {
        self.is_status_modal_open = true;
        self.selected_status_idx = 0;
    }

    pub fn close_status_modal(&mut self) {
        self.is_status_modal_open = false;
    }

    pub fn status_up(&mut self) {
        if self.selected_status_idx == 0 {
            self.selected_status_idx = STATUS_OPTIONS.len() - 1;
        } else {
            self.selected_status_idx -= 1;
        }
    }

    pub fn status_down(&mut self) {
        self.selected_status_idx = (self.selected_status_idx + 1) % STATUS_OPTIONS.len();
    }

    pub fn search_char(&mut self, c: char) {
        self.search_query.push(c);
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
    }

    pub fn search_down(&mut self) {
        if !self.search_results.is_empty() {
            self.selected_search_idx = (self.selected_search_idx + 1) % self.search_results.len();
        }
    }

    pub fn search_up(&mut self) {
        if !self.search_results.is_empty() {
            if self.selected_search_idx == 0 {
                self.selected_search_idx = self.search_results.len() - 1;
            } else {
                self.selected_search_idx -= 1;
            }
        }
    }

    pub fn selected_search_user(&self) -> Option<&OrgUser> {
        self.search_results.get(self.selected_search_idx)
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

    pub fn insert_char(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 && !self.input_buffer.is_empty() {
            self.cursor_pos -= 1;
            self.input_buffer.remove(self.cursor_pos);
        }
    }

    pub fn clear_input(&mut self) -> String {
        let text = self.input_buffer.clone();
        self.input_buffer.clear();
        self.cursor_pos = 0;
        text
    }

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
                title: format!("{} > #{}", team_name, channel.display_name),
            }),
            _ => None,
        }
    }

    pub fn get_cached_messages(&self, conv: &ActiveConversation) -> Option<&Vec<ChatMessage>> {
        self.message_cache.get(&conv.cache_key())
    }

    pub fn cache_messages(&mut self, conv: &ActiveConversation, msgs: Vec<ChatMessage>) {
        self.message_cache.insert(conv.cache_key(), msgs);
    }

    /// Search local contacts from existing chats and teams instantly (0ms latency, works offline / without directory permissions)
    pub fn search_local_contacts(&self, query: &str) -> Vec<OrgUser> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }

        let mut seen = std::collections::HashSet::new();
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
                        if name.to_lowercase().contains(&q) || email.to_lowercase().contains(&q) {
                            if seen.insert(name.to_lowercase()) {
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
        }

        results
    }

    /// Find an existing 1:1 chat for a given user if already in chat list
    pub fn find_existing_chat(&self, target_name: &str, target_id: &str) -> Option<ChatSummary> {
        for chat in &self.raw_chats {
            if !target_id.is_empty() {
                if chat.members.iter().any(|m| {
                    m.user_id.as_deref() == Some(target_id) || m.id.as_deref() == Some(target_id)
                }) {
                    return Some(chat.clone());
                }
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
                        team_name: team.display_name.clone(),
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
