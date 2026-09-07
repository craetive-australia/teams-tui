use crate::app::{ActiveConversation, App, FocusedPane, SidebarItem};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame, app: &App) {
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header Bar
            Constraint::Min(8),    // Body (Sidebar + Message view)
            Constraint::Length(3), // Input Bar
            Constraint::Length(1), // Footer / Status Bar
        ])
        .split(frame.area());

    render_header(frame, app, main_layout[0]);

    let body_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // Sidebar
            Constraint::Percentage(70), // Messages
        ])
        .split(main_layout[1]);

    render_sidebar(frame, app, body_layout[0]);
    render_messages(frame, app, body_layout[1]);
    render_input(frame, app, main_layout[2]);
    render_footer(frame, app, main_layout[3]);

    if app.is_search_modal_open {
        render_search_modal(frame, app);
    } else if app.is_status_modal_open {
        render_status_modal(frame, app);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let user_name = app
        .current_user
        .as_ref()
        .and_then(|u| u.display_name.clone())
        .unwrap_or_else(|| "Connecting...".to_string());

    let (presence_icon, presence_color, presence_text) = match app.presence.as_ref() {
        Some(p) => match p.availability.to_lowercase().as_str() {
            "available" => ("🟢", Color::Green, format!("Available ({})", p.activity)),
            "busy" => ("🔴", Color::Red, format!("Busy ({})", p.activity)),
            "donotdisturb" => ("⛔", Color::Red, "Do Not Disturb".to_string()),
            "away" | "berightback" => ("🟡", Color::Yellow, "Away".to_string()),
            "offline" => ("⚪", Color::DarkGray, "Offline".to_string()),
            _ => ("🔵", Color::Cyan, p.availability.clone()),
        },
        None => ("⚪", Color::DarkGray, "Unknown".to_string()),
    };

    let title_spans = vec![
        Span::styled("Teams TUI ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!("| User: {} ", user_name), Style::default().fg(Color::White)),
        Span::styled(format!("| {} ", presence_icon), Style::default().fg(presence_color)),
        Span::styled(presence_text, Style::default().fg(presence_color).add_modifier(Modifier::BOLD)),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let header_paragraph = Paragraph::new(Line::from(title_spans))
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(header_paragraph, area);
}

fn render_sidebar(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Sidebar;
    let border_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    let items: Vec<ListItem> = app
        .sidebar_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let is_selected = idx == app.selected_sidebar_idx;
            match item {
                SidebarItem::SectionHeader(title) => {
                    ListItem::new(Line::from(vec![Span::styled(
                        title.clone(),
                        Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
                    )]))
                }
                SidebarItem::Chat { title, chat } => {
                    let is_active = matches!(&app.active_conversation, Some(ActiveConversation::Chat { id, .. }) if id == &chat.id);
                    let marker = if is_selected { "▶ " } else { "  " };
                    
                    let mut presence_icon = if is_active { "● " } else { "  " };
                    let mut presence_color = Color::Green;

                    if chat.chat_type == "oneOnOne" {
                        let my_id = app.current_user.as_ref().map(|u| u.id.as_str());
                        for m in &chat.members {
                            if let Some(uid) = &m.user_id {
                                if Some(uid.as_str()) != my_id {
                                    if let Some(p) = app.user_presences.get(uid) {
                                        let (icon, color) = match p.availability.to_lowercase().as_str() {
                                            "available" => ("🟢", Color::Green),
                                            "busy" | "donotdisturb" => ("🔴", Color::Red),
                                            "away" | "berightback" => ("🟡", Color::Yellow),
                                            "offline" => ("⚪", Color::DarkGray),
                                            _ => ("🔵", Color::Cyan),
                                        };
                                        presence_icon = icon;
                                        presence_color = color;
                                    }
                                    break;
                                }
                            }
                        }
                    }

                    let mut style = Style::default().fg(Color::White);
                    if is_selected {
                        style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                    }
                    if is_active {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }

                    ListItem::new(Line::from(vec![
                        Span::styled(marker, Style::default().fg(Color::Yellow)),
                        Span::styled(presence_icon, Style::default().fg(presence_color)),
                        Span::styled(format!(" {}", title), style),
                    ]))
                }
                SidebarItem::TeamHeader { team, .. } => {
                    ListItem::new(Line::from(vec![
                        Span::styled("▾ ", Style::default().fg(Color::Cyan)),
                        Span::styled(team.display_name.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    ]))
                }
                SidebarItem::Channel {
                    channel,
                    team_id,
                    ..
                } => {
                    let is_active = matches!(&app.active_conversation, Some(ActiveConversation::Channel { channel_id, team_id: tid, .. }) if channel_id == &channel.id && tid == team_id);
                    let marker = if is_selected { "  ▶ " } else { "    " };
                    let active_icon = if is_active { "● " } else { "  " };

                    let mut style = Style::default().fg(Color::Gray);
                    if is_selected {
                        style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD);
                    }
                    if is_active {
                        style = style.add_modifier(Modifier::UNDERLINED);
                    }

                    ListItem::new(Line::from(vec![
                        Span::styled(marker, Style::default().fg(Color::Yellow)),
                        Span::styled(active_icon, Style::default().fg(Color::Green)),
                        Span::styled(format!("#{}", channel.display_name), style),
                    ]))
                }
            }
        })
        .collect();

    let list_title = if is_focused { " [Chats & Teams (Active)] " } else { " Chats & Teams " };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(list_title)
            .border_style(Style::default().fg(border_color)),
    );

    frame.render_widget(list, area);
}

fn render_messages(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Messages;
    let border_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    let mut conv_title = match &app.active_conversation {
        Some(ActiveConversation::Chat { title, .. }) => format!(" Chat: {} ", title),
        Some(ActiveConversation::Channel { title, .. }) => format!(" Channel: {} ", title),
        None => " No conversation selected ".to_string(),
    };

    if app.is_syncing {
        conv_title.push_str("[Syncing...] ");
    }

    let title = if is_focused {
        format!("{} (Active - Scroll with ↑/↓) ", conv_title.trim())
    } else {
        conv_title
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));

    if app.messages.is_empty() {
        let msg = if app.is_loading {
            if matches!(app.active_conversation, Some(ActiveConversation::Channel { .. })) {
                "Loading channel messages from Microsoft Graph... (Channels may take a few moments)"
            } else {
                "Loading messages..."
            }
        } else if app.active_conversation.is_none() {
            "Select a chat or channel from the sidebar and press Enter."
        } else {
            "No messages found in this conversation."
        };
        let p = Paragraph::new(msg)
            .block(block)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(p, area);
        return;
    }

    // Graph API returns messages in reverse-chronological order (newest first).
    // For natural reading in chat, we display chronologically (oldest at top, newest at bottom).
    let mut lines = Vec::new();

    for msg in app.messages.iter().rev() {
        let sender = msg.sender_display_name();
        let timestamp = msg
            .created_at
            .as_ref()
            .and_then(|t| {
                chrono::DateTime::parse_from_rfc3339(t).ok().map(|dt| {
                    dt.with_timezone(&chrono::Local)
                        .format("%H:%M:%S")
                        .to_string()
                })
            })
            .unwrap_or_else(|| "--:--:--".to_string());

        let is_me = app
            .current_user
            .as_ref()
            .and_then(|u| u.display_name.as_deref())
            .map(|name| name.eq_ignore_ascii_case(&sender))
            .unwrap_or(false);

        let sender_style = if is_me {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("[{}] ", timestamp), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}:", sender), sender_style),
        ]));

        let content = msg.clean_content();
        for line in content.lines() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line.to_string(), Style::default().fg(Color::White)),
            ]));
        }

        if !msg.reactions.is_empty() {
            let mut reaction_spans = vec![Span::raw("    ")];
            for r in &msg.reactions {
                let emoji = match r.reaction_type.to_lowercase().as_str() {
                    "like" => "👍",
                    "heart" => "❤️",
                    "laugh" => "😆",
                    "surprised" => "😮",
                    "sad" => "😢",
                    "angry" => "😡",
                    _ => "✨",
                };
                reaction_spans.push(Span::styled(format!("{} ", emoji), Style::default().fg(Color::Yellow)));
            }
            lines.push(Line::from(reaction_spans));
        }

        // Blank line between messages
        lines.push(Line::raw(""));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.message_scroll as u16, 0));

    frame.render_widget(p, area);
}

fn render_input(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focused_pane == FocusedPane::Input;
    let border_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    let target_name = match &app.active_conversation {
        Some(ActiveConversation::Chat { title, .. }) => title.as_str(),
        Some(ActiveConversation::Channel { title, .. }) => title.as_str(),
        None => "None (Select a conversation first)",
    };

    let title = if is_focused {
        format!(" Message to: {} (Press Enter to Send, Esc to Exit) ", target_name)
    } else {
        format!(" Message: {} (Press 'i' to type) ", target_name)
    };

    let input_text = format!("> {}", app.input_buffer);
    let p = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(border_color)),
        )
        .style(if is_focused {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    frame.render_widget(p, area);

    // Set terminal cursor in input box when focused
    if is_focused {
        let cursor_x = area.x + 3 + app.cursor_pos as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status_span = Span::styled(
        format!(" Status: {} ", app.status_message),
        Style::default().fg(Color::Black).bg(Color::Cyan),
    );

    let help_text = match app.focused_pane {
        FocusedPane::Sidebar => " [j/k] Navigate | [Enter] Open Chat | [s] Search | [p] Status | [r] Refresh | [q] Quit ",
        FocusedPane::Messages => " [↑/↓] Scroll Messages | [i] Type Message | [/] Search Users | [Esc] Back to Sidebar ",
        FocusedPane::Input => " [Enter] Send | [Esc] Cancel/Unfocus | [Tab] Switch Pane ",
    };

    let footer_line = Line::from(vec![
        status_span,
        Span::raw(" "),
        Span::styled(help_text, Style::default().fg(Color::DarkGray)),
    ]);

    let footer = Paragraph::new(footer_line);
    frame.render_widget(footer, area);
}

fn render_search_modal(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search input bar
            Constraint::Min(4),    // Search results list
            Constraint::Length(1), // Modal footer
        ])
        .split(area);

    // Search input block
    let search_title = if app.is_searching {
        " Search Users (Searching directory & contacts...) "
    } else if app.directory_search_denied {
        " Search Recent Contacts (Run 'teams-tui --login' for full directory) "
    } else {
        " Search Organisation Users & Contacts (Type Name or Email) "
    };

    let search_input = Paragraph::new(format!("> {}", app.search_query))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(search_title)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(search_input, chunks[0]);

    // Position cursor in search box
    let cursor_x = chunks[0].x + 3 + app.search_query.len() as u16;
    let cursor_y = chunks[0].y + 1;
    if cursor_x < chunks[0].x + chunks[0].width - 1 {
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // Results list
    let results_title = if let Some(ref err) = app.search_error_msg {
        format!(" Results ({}) [Note: {}] ", app.search_results.len(), err)
    } else {
        format!(" Results ({}) ", app.search_results.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(results_title)
        .border_style(Style::default().fg(Color::Cyan));

    if app.search_results.is_empty() {
        let msg = if let Some(ref err) = app.search_error_msg {
            err.as_str()
        } else if app.search_query.trim().is_empty() {
            "Type at least 1 character to search users in the organisation or recent contacts."
        } else if app.is_searching {
            "Searching directory and recent contacts..."
        } else {
            "No matching users or contacts found."
        };
        let empty_p = Paragraph::new(msg)
            .block(block)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(empty_p, chunks[1]);
    } else {
        let items: Vec<ListItem> = app
            .search_results
            .iter()
            .enumerate()
            .map(|(idx, user)| {
                let is_selected = idx == app.selected_search_idx;
                let prefix = if is_selected { "▶ " } else { "  " };

                let name = user.display_name.as_deref().unwrap_or("Unknown");
                let role = user.job_title.as_deref().unwrap_or("");
                let email = user
                    .mail
                    .as_deref()
                    .or(user.user_principal_name.as_deref())
                    .unwrap_or("No email");
                let dept = user.department.as_deref().unwrap_or("");

                let name_style = if is_selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                };

                let subtext = if !dept.is_empty() {
                    format!("{} • {}", email, dept)
                } else {
                    email.to_string()
                };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(prefix, Style::default().fg(Color::Yellow)),
                        Span::styled(name, name_style),
                        if !role.is_empty() {
                            Span::styled(format!(" ({})", role), Style::default().fg(Color::DarkGray))
                        } else {
                            Span::raw("")
                        },
                    ]),
                    Line::from(vec![
                        Span::raw("    "),
                        Span::styled(subtext, Style::default().fg(Color::Gray)),
                    ]),
                ])
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, chunks[1]);
    }

    // Modal footer
    let help_line = Line::from(vec![
        Span::styled(
            " [Enter] Start 1:1 Chat | [↑/↓] Select User | [Esc] Close Search ",
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ),
    ]);
    let footer_p = Paragraph::new(help_line).alignment(Alignment::Center);
    frame.render_widget(footer_p, chunks[2]);
}

fn render_status_modal(frame: &mut Frame, app: &App) {
    let area = centered_rect(40, 40, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Set Presence Status ")
        .border_style(Style::default().fg(Color::Cyan));

    let items: Vec<ListItem> = crate::app::STATUS_OPTIONS
        .iter()
        .enumerate()
        .map(|(idx, (label, icon, _, _))| {
            let is_selected = idx == app.selected_status_idx;
            let prefix = if is_selected { "▶ " } else { "  " };
            let style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Yellow)),
                Span::styled(format!("{} ", icon), Style::default()),
                Span::styled(*label, style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
