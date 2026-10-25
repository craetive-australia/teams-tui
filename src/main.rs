mod app;
mod auth;
mod config;
mod graph;
mod logger;
mod ui;

use app::{ActiveConversation, App, FocusedPane};
use auth::AuthManager;
use clap::Parser;
use config::{AppConfig, CliArgs};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use graph::{ChannelSummary, ChatMessage, ChatSummary, GraphClient, OrgUser, PresenceInfo, TeamSummary};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// RAII Guard that ensures terminal raw mode and alternate screen are restored on normal exit or panic
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, io::Error> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            default_hook(info);
        }));

        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Debug)]
enum BackgroundEvent {
    TeamChannelsLoaded {
        team_id: String,
        channels: Vec<ChannelSummary>,
    },
    MessagesLoaded {
        conv: ActiveConversation,
        messages: Vec<ChatMessage>,
        elapsed_ms: u128,
    },
    MessageLoadFailed {
        conv: ActiveConversation,
        error: String,
    },
    MessageSent {
        conv: ActiveConversation,
        elapsed_ms: u128,
    },
    MessageSendFailed {
        error: String,
    },
    SearchResults {
        query: String,
        results: Vec<OrgUser>,
        elapsed_ms: u128,
    },
    SearchFailed {
        error: String,
    },
    ChatCreated {
        chat: ChatSummary,
        target_name: String,
    },
    ChatCreateFailed {
        error: String,
    },
    PresenceUpdated(PresenceInfo),
    ConversationPolled {
        conv: ActiveConversation,
        messages: Vec<ChatMessage>,
    },
    SidebarRefreshed {
        chats: Vec<ChatSummary>,
        teams: Vec<(TeamSummary, Vec<ChannelSummary>)>,
        elapsed_ms: u128,
    },
    UserPresencesLoaded(Vec<(String, PresenceInfo)>),
    PresenceSetResult(Result<(), String>),
}

fn extract_user_ids(chats: &[ChatSummary], current_user_id: Option<&str>) -> Vec<String> {
    let mut ids = std::collections::HashSet::new();
    for chat in chats {
        if chat.chat_type == "oneOnOne" {
            for m in &chat.members {
                if let Some(uid) = &m.user_id {
                    if Some(uid.as_str()) != current_user_id {
                        ids.insert(uid.clone());
                    }
                }
            }
        }
    }
    ids.into_iter().collect()
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = CliArgs::parse();
    let config = AppConfig::load(&args);

    // Initialize File Logger
    logger::init(&config.log_file, config.debug);

    println!("=======================================================");
    println!("             Teams TUI (Microsoft Teams)               ");
    println!("=======================================================");
    println!("Client ID : {}", config.client_id);
    println!("Tenant ID : {}", config.tenant_id);
    println!("Log File  : {}", config.log_file);
    println!("Debug Mode: {}", config.debug);
    println!("Insecure Token File: {}", config.allow_insecure_token_file);
    println!("Config    : {}", AppConfig::default_config_path().display());
    println!("=======================================================");

    // Authenticate
    let auth = Arc::new(AuthManager::new(
        config.client_id.clone(),
        config.tenant_id.clone(),
        config.allow_insecure_token_file,
    )?);

    let access_token = match auth.get_access_token(args.login).await {
        Ok(token) => token,
        Err(e) => {
            logger::log_error(format!("Authentication error: {e}"));
            eprintln!("\nAuthentication error: {e}");
            eprintln!("\nIf your organization requires admin consent for the public client ID,");
            eprintln!("register an app in Microsoft Entra ID and run:");
            eprintln!("  teams-tui --client-id <YOUR_CLIENT_ID> --tenant-id <YOUR_TENANT_ID>\n");
            return Err(e);
        }
    };

    println!("Fetching initial conversations (concurrent)...");
    let start_init = Instant::now();
    let graph = Arc::new(GraphClient::new(access_token, Some(auth.clone())));
    let mut app = App::new();

    // Concurrent initial data fetch
    let (profile_res, presence_res, chats_res, teams_res) = tokio::join!(
        graph.get_me(),
        graph.get_presence(),
        graph.get_my_chats(),
        graph.get_joined_teams()
    );

    if let Ok(profile) = profile_res {
        println!("Signed in as: {}", profile.display_name.as_deref().unwrap_or("User"));
        logger::log_info(format!("Signed in user: {:?}", profile.display_name));
        app.current_user = Some(profile);
    }
    if let Ok(presence) = presence_res {
        app.presence = Some(presence);
    }

    let chats = chats_res.unwrap_or_default();
    let teams_list = teams_res.unwrap_or_default();

    logger::log_info(format!(
        "Fetched metadata in {}ms ({} chats, {} teams). Launching UI immediately...",
        start_init.elapsed().as_millis(),
        chats.len(),
        teams_list.len()
    ));

    // Initialize sidebar immediately with teams (channels will load asynchronously in background)
    let initial_teams: Vec<(TeamSummary, Vec<ChannelSummary>)> = teams_list
        .iter()
        .map(|team| (team.clone(), Vec::new()))
        .collect();

    app.rebuild_sidebar(&chats, &initial_teams);

    // Initial conversation selection
    if let Some(initial_conv) = app.currently_selected_conversation() {
        app.active_conversation = Some(initial_conv);
    }

    // Set up Async Background Event Channel
    let (bg_tx, mut bg_rx) = mpsc::channel::<BackgroundEvent>(100);
    let active_fetch_in_progress = Arc::new(AtomicBool::new(false));

    // Asynchronously load channels for each team in the background
    for team in &teams_list {
        let g = graph.clone();
        let tx = bg_tx.clone();
        let t_id = team.id.clone();
        tokio::spawn(async move {
            let channels = g.get_team_channels(&t_id).await.unwrap_or_default();
            let _ = tx.send(BackgroundEvent::TeamChannelsLoaded {
                team_id: t_id,
                channels,
            }).await;
        });
    }

    // If an initial conversation was selected, trigger initial background load
    if let Some(ref conv) = app.active_conversation {
        app.is_loading = true;
        trigger_load_conversation(bg_tx.clone(), graph.clone(), conv.clone(), active_fetch_in_progress.clone());
    }

    // Trigger initial background presence load for visible 1:1 chats
    {
        let initial_users = extract_user_ids(&app.raw_chats, app.current_user.as_ref().map(|u| u.id.as_str()));
        trigger_load_presences(bg_tx.clone(), graph.clone(), initial_users);
    }

    // Spawn dedicated background presence session keeper & poller
    {
        let bg_tx_poller = bg_tx.clone();
        let graph_poller = graph.clone();
        let poll_interval = Duration::from_secs(config.poll_interval_secs.max(3));
        let client_id = config.client_id.clone();

        tokio::spawn(async move {
            if let Err(e) = graph_poller.establish_presence_session(&client_id).await {
                logger::log_error(format!("Failed to establish presence session: {e}"));
            }

            let mut last_session_refresh = Instant::now();
            let session_refresh_interval = Duration::from_mins(45);

            loop {
                tokio::time::sleep(poll_interval).await;

                if last_session_refresh.elapsed() >= session_refresh_interval {
                    if let Err(e) = graph_poller.establish_presence_session(&client_id).await {
                        logger::log_error(format!("Failed to refresh presence session: {e}"));
                    }
                    last_session_refresh = Instant::now();
                }

                if let Ok(presence) = graph_poller.get_presence().await {
                    let _ = bg_tx_poller.send(BackgroundEvent::PresenceUpdated(presence)).await;
                }
            }
        });
    }

    // Terminal Initialization with RAII Guard
    let mut guard = TerminalGuard::new()?;

    let mut last_search_query = String::new();
    let mut search_debounce_timer: Option<Instant> = None;
    let mut last_active_poll = Instant::now();
    let mut last_presence_poll = Instant::now();
    let active_poll_interval_chat = Duration::from_secs(config.poll_interval_secs.max(5));
    let active_poll_interval_channel = Duration::from_secs(config.poll_interval_secs.max(15));
    let presence_poll_interval = Duration::from_secs(30);

    logger::log_info("Starting UI event loop...");

    loop {
        // 1. Process pending background events
        while let Ok(event) = bg_rx.try_recv() {
            handle_background_event(
                &mut app,
                event,
                &bg_tx,
                &graph,
                &active_fetch_in_progress,
            );
        }

        // 2. Render frame
        guard.terminal.draw(|f| ui::render(f, &app))?;

        // 3. Debounced user directory search
        if app.search.is_open {
            if let Some(timer) = search_debounce_timer {
                if timer.elapsed() >= Duration::from_millis(300) {
                    search_debounce_timer = None;
                    let q = app.search.query.trim().to_string();
                    if !q.is_empty() && q != last_search_query && !app.search.directory_search_denied {
                        last_search_query.clone_from(&q);
                        app.search.is_searching = true;
                        let tx = bg_tx.clone();
                        let g = graph.clone();
                        tokio::spawn(async move {
                            let t0 = Instant::now();
                            match g.search_users(&q).await {
                                Ok(results) => {
                                    let _ = tx.send(BackgroundEvent::SearchResults {
                                        query: q,
                                        results,
                                        elapsed_ms: t0.elapsed().as_millis(),
                                    }).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(BackgroundEvent::SearchFailed {
                                        error: e.to_string(),
                                    }).await;
                                }
                            }
                        });
                    }
                }
            }
        }

        // 4. Background polling for active conversation
        let is_channel = matches!(app.active_conversation, Some(ActiveConversation::Channel { .. }));
        let poll_interval = if is_channel {
            active_poll_interval_channel
        } else {
            active_poll_interval_chat
        };

        if last_active_poll.elapsed() >= poll_interval {
            last_active_poll = Instant::now();
            if !active_fetch_in_progress.load(Ordering::Relaxed) {
                if let Some(conv) = app.active_conversation.clone() {
                    let tx = bg_tx.clone();
                    let g = graph.clone();
                    let progress_guard = active_fetch_in_progress.clone();
                    progress_guard.store(true, Ordering::Relaxed);
                    tokio::spawn(async move {
                        let res = match &conv {
                            ActiveConversation::Chat { id, .. } => g.get_chat_messages(id).await,
                            ActiveConversation::Channel { team_id, channel_id, .. } => {
                                g.get_channel_messages(team_id, channel_id).await
                            }
                        };
                        progress_guard.store(false, Ordering::Relaxed);
                        if let Ok(msgs) = res {
                            let _ = tx.send(BackgroundEvent::ConversationPolled { conv, messages: msgs }).await;
                        }
                    });
                }
            }
        }

        if last_presence_poll.elapsed() >= presence_poll_interval {
            last_presence_poll = Instant::now();
            let user_ids = extract_user_ids(&app.raw_chats, app.current_user.as_ref().map(|u| u.id.as_str()));
            trigger_load_presences(bg_tx.clone(), graph.clone(), user_ids);
        }

        // 5. Poll keyboard events (30ms for ~33 FPS responsive UI)
        if event::poll(Duration::from_millis(30))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                logger::log_debug(format!("Key pressed: {:?}", key.code));

                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }

                if app.search.is_open {
                    handle_search_modal_key(
                        &mut app,
                        key,
                        &mut search_debounce_timer,
                        &mut last_search_query,
                        &bg_tx,
                        &graph,
                        &active_fetch_in_progress,
                    );
                    continue;
                }

                if app.status_modal.is_open {
                    handle_status_modal_key(&mut app, key, &bg_tx, &graph);
                    continue;
                }

                handle_pane_key(
                    &mut app,
                    key,
                    &bg_tx,
                    &graph,
                    &active_fetch_in_progress,
                );
            }
        }

        if app.should_quit {
            break;
        }
    }

    logger::log_info("Teams TUI session closed cleanly.");
    println!("Teams TUI exited cleanly. Goodbye!");
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn handle_background_event(
    app: &mut App,
    event: BackgroundEvent,
    bg_tx: &mpsc::Sender<BackgroundEvent>,
    graph: &Arc<GraphClient>,
    active_fetch_in_progress: &Arc<AtomicBool>,
) {
    match event {
        BackgroundEvent::TeamChannelsLoaded { team_id, channels } => {
            logger::log_info(format!("Loaded {} channels for team {team_id}", channels.len()));
            app.update_team_channels(&team_id, channels);
        }
        BackgroundEvent::MessagesLoaded { conv, messages, elapsed_ms } => {
            app.cache_messages(&conv, messages.clone());
            if app.active_conversation.as_ref() == Some(&conv) {
                logger::log_info(format!(
                    "Loaded {} messages for {:?} in {elapsed_ms}ms",
                    messages.len(),
                    conv,
                ));
                app.messages = messages;
                app.is_loading = false;
                app.is_syncing = false;
                app.set_status(format!("Loaded ({elapsed_ms}ms)"));
            }
        }
        BackgroundEvent::MessageLoadFailed { conv, error } => {
            if app.active_conversation.as_ref() == Some(&conv) {
                logger::log_error(format!("Failed to load messages for {conv:?}: {error}"));
                app.is_loading = false;
                app.is_syncing = false;
                app.set_status(format!("Load failed: {error}"));
            }
        }
        BackgroundEvent::MessageSent { conv, elapsed_ms } => {
            logger::log_info(format!("Message sent in {elapsed_ms}ms. Reloading conversation..."));
            app.set_status(format!("Sent ({elapsed_ms}ms)"));
            trigger_load_conversation(bg_tx.clone(), graph.clone(), conv, active_fetch_in_progress.clone());
        }
        BackgroundEvent::MessageSendFailed { error } => {
            logger::log_error(format!("Message send failed: {error}"));
            app.set_status(format!("Send failed: {error}"));
        }
        BackgroundEvent::SearchResults { query, results, elapsed_ms } => {
            if app.search.is_open && app.search.query == query {
                logger::log_info(format!(
                    "Search for '{query}' returned {} remote results in {elapsed_ms}ms",
                    results.len()
                ));
                let mut combined = app.search_local_contacts(&query);
                for r in results {
                    if !combined.iter().any(|c| c.id == r.id || (c.display_name.is_some() && c.display_name == r.display_name)) {
                        combined.push(r);
                    }
                }
                app.search.results = combined;
                app.search.is_searching = false;
            }
        }
        BackgroundEvent::SearchFailed { error } => {
            logger::log_warn(format!("Search failed: {error}"));
            app.search.is_searching = false;
            if error.contains("PERMISSION_DENIED") || error.contains("403") {
                app.search.directory_search_denied = true;
                app.search.error_msg = Some("Directory search permission required. Run 'teams-tui --login'. Showing local contacts:".to_string());
            } else {
                app.search.error_msg = Some(format!("Search error: {error}"));
            }
        }
        BackgroundEvent::ChatCreated { chat, target_name } => {
            let new_conv = ActiveConversation::Chat {
                id: chat.id.clone(),
                title: target_name.clone(),
            };
            app.active_conversation = Some(new_conv.clone());
            app.close_search();
            app.focused_pane = FocusedPane::Input;
            app.set_status(format!("Chatting with {target_name}. Type a message and press Enter!"));

            trigger_load_conversation(bg_tx.clone(), graph.clone(), new_conv, active_fetch_in_progress.clone());
            trigger_refresh_sidebar(bg_tx.clone(), graph.clone());
        }
        BackgroundEvent::ChatCreateFailed { error } => {
            logger::log_error(format!("Failed to create 1:1 chat: {error}"));
            app.set_status(format!("Could not start chat: {error}"));
        }
        BackgroundEvent::PresenceUpdated(presence) => {
            app.presence = Some(presence);
        }
        BackgroundEvent::ConversationPolled { conv, messages } => {
            app.cache_messages(&conv, messages.clone());
            if app.active_conversation.as_ref() == Some(&conv) {
                app.messages = messages;
            }
        }
        BackgroundEvent::SidebarRefreshed { chats, teams, elapsed_ms } => {
            logger::log_info(format!("Sidebar refreshed in {elapsed_ms}ms"));
            app.rebuild_sidebar(&chats, &teams);
            app.set_status("Refreshed!");
        }
        BackgroundEvent::UserPresencesLoaded(presences) => {
            for (id, p) in presences {
                app.user_presences.insert(id, p);
            }
        }
        BackgroundEvent::PresenceSetResult(res) => match res {
            Ok(()) => app.set_status("Presence updated successfully."),
            Err(e) => app.set_status(format!("Failed to update presence: {e}")),
        },
    }
}

fn handle_search_modal_key(
    app: &mut App,
    key: KeyEvent,
    search_debounce_timer: &mut Option<Instant>,
    last_search_query: &mut String,
    bg_tx: &mpsc::Sender<BackgroundEvent>,
    graph: &Arc<GraphClient>,
    active_fetch_in_progress: &Arc<AtomicBool>,
) {
    match key.code {
        KeyCode::Esc => {
            app.close_search();
        }
        KeyCode::Down | KeyCode::Tab => {
            app.search_down();
        }
        KeyCode::Up | KeyCode::BackTab => {
            app.search_up();
        }
        KeyCode::Backspace => {
            app.search_backspace();
            *search_debounce_timer = Some(Instant::now());
            if app.search.query.trim().is_empty() {
                app.search.results.clear();
                last_search_query.clear();
            } else {
                let local = app.search_local_contacts(&app.search.query);
                app.search.results = local;
                app.search.selected_idx = 0;
            }
        }
        KeyCode::Char(c) => {
            app.search_char(c);
            *search_debounce_timer = Some(Instant::now());
            let local = app.search_local_contacts(&app.search.query);
            if !local.is_empty() {
                app.search.results = local;
                app.search.selected_idx = 0;
            }
        }
        KeyCode::Enter => {
            if let Some(target_user) = app.selected_search_user().cloned() {
                let target_id = target_user.id.clone();
                let target_name = target_user.display_name.clone().unwrap_or_else(|| "User".to_string());
                let my_id = app.current_user.as_ref().map(|u| u.id.clone()).unwrap_or_default();

                if let Some(existing_chat) = app.find_existing_chat(&target_name, &target_id) {
                    let new_conv = ActiveConversation::Chat {
                        id: existing_chat.id.clone(),
                        title: target_name.clone(),
                    };
                    app.active_conversation = Some(new_conv.clone());
                    app.close_search();
                    app.focused_pane = FocusedPane::Input;
                    app.set_status(format!("Opened chat with {target_name}. Press 'i' or Enter to type!"));

                    if let Some(cached) = app.get_cached_messages(&new_conv) {
                        app.messages = cached.clone();
                        app.is_loading = false;
                        app.is_syncing = true;
                    } else {
                        app.messages.clear();
                        app.is_loading = true;
                        app.is_syncing = false;
                    }

                    trigger_load_conversation(bg_tx.clone(), graph.clone(), new_conv, active_fetch_in_progress.clone());
                } else if !my_id.is_empty() && !target_id.is_empty() {
                    app.set_status(format!("Opening 1:1 chat with {target_name}..."));
                    let g = graph.clone();
                    let tx = bg_tx.clone();
                    let t_name = target_name;

                    tokio::spawn(async move {
                        match g.get_or_create_1on1_chat(&my_id, &target_id).await {
                            Ok(chat) => {
                                let _ = tx.send(BackgroundEvent::ChatCreated {
                                    chat,
                                    target_name: t_name,
                                }).await;
                            }
                            Err(e) => {
                                let _ = tx.send(BackgroundEvent::ChatCreateFailed {
                                    error: e.to_string(),
                                }).await;
                            }
                        }
                    });
                }
            }
        }
        _ => {}
    }
}

fn handle_status_modal_key(
    app: &mut App,
    key: KeyEvent,
    bg_tx: &mpsc::Sender<BackgroundEvent>,
    graph: &Arc<GraphClient>,
) {
    match key.code {
        KeyCode::Esc => {
            app.close_status_modal();
        }
        KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
            app.status_down();
        }
        KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
            app.status_up();
        }
        KeyCode::Enter => {
            let (_, _, availability, activity) = app::STATUS_OPTIONS[app.status_modal.selected_idx];
            let my_id = app.current_user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
            if !my_id.is_empty() {
                let g = graph.clone();
                let tx = bg_tx.clone();
                let avail = availability.to_string();
                let act = activity.to_string();

                tokio::spawn(async move {
                    logger::log_info(format!("Setting presence to: {avail} ({act})"));
                    let res = if avail.is_empty() {
                        g.clear_preferred_presence(&my_id).await
                    } else {
                        g.set_preferred_presence(&my_id, &avail, &act).await
                    };

                    match res {
                        Ok(()) => {
                            logger::log_info("Successfully updated preferred presence.");
                            let _ = tx.send(BackgroundEvent::PresenceSetResult(Ok(()))).await;
                            if let Ok(presence) = g.get_presence().await {
                                let _ = tx.send(BackgroundEvent::PresenceUpdated(presence)).await;
                            }
                        }
                        Err(e) => {
                            logger::log_error(format!("Failed to set preferred presence: {e}"));
                            let _ = tx.send(BackgroundEvent::PresenceSetResult(Err(e.to_string()))).await;
                        }
                    }
                });
            }
            app.close_status_modal();
            app.set_status("Updating presence status...");
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
fn handle_pane_key(
    app: &mut App,
    key: KeyEvent,
    bg_tx: &mpsc::Sender<BackgroundEvent>,
    graph: &Arc<GraphClient>,
    active_fetch_in_progress: &Arc<AtomicBool>,
) {
    match app.focused_pane {
        FocusedPane::Input => match key.code {
            KeyCode::Enter => {
                if !app.input_buffer.trim().is_empty() {
                    if let Some(conv) = app.active_conversation.clone() {
                        let content = app.clear_input();
                        app.set_status("Sending message...");

                        let tx = bg_tx.clone();
                        let g = graph.clone();
                        let c = conv;

                        tokio::spawn(async move {
                            let t0 = Instant::now();
                            let res = match &c {
                                ActiveConversation::Chat { id, .. } => {
                                    g.send_chat_message(id, &content).await
                                }
                                ActiveConversation::Channel { team_id, channel_id, .. } => {
                                    g.send_channel_message(team_id, channel_id, &content).await
                                }
                            };
                            let elapsed_ms = t0.elapsed().as_millis();
                            match res {
                                Ok(()) => {
                                    let _ = tx.send(BackgroundEvent::MessageSent { conv: c, elapsed_ms }).await;
                                }
                                Err(e) => {
                                    let _ = tx.send(BackgroundEvent::MessageSendFailed { error: e.to_string() }).await;
                                }
                            }
                        });
                    } else {
                        app.set_status("Cannot send: No active conversation selected.");
                    }
                }
            }
            KeyCode::Backspace => {
                app.backspace();
            }
            KeyCode::Esc => {
                app.focused_pane = FocusedPane::Sidebar;
            }
            KeyCode::Tab => {
                app.cycle_focus();
            }
            KeyCode::Left => {
                app.move_cursor_left();
            }
            KeyCode::Right => {
                app.move_cursor_right();
            }
            KeyCode::Char(c) => {
                app.insert_char(c);
            }
            _ => {}
        },

        FocusedPane::Sidebar => match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Tab => app.cycle_focus(),
            KeyCode::Char('i') => {
                app.focused_pane = FocusedPane::Input;
            }
            KeyCode::Char('/' | 's') => {
                app.open_search();
            }
            KeyCode::Char('p') => {
                app.open_status_modal();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.sidebar_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.sidebar_up();
            }
            KeyCode::Enter => {
                if let Some(conv) = app.currently_selected_conversation() {
                    let title = match &conv {
                        ActiveConversation::Chat { title, .. } | ActiveConversation::Channel { title, .. } => {
                            title.clone()
                        }
                    };
                    app.active_conversation = Some(conv.clone());

                    if let Some(cached) = app.get_cached_messages(&conv) {
                        app.messages = cached.clone();
                        app.is_loading = false;
                        app.is_syncing = true;
                        app.set_status(format!("Loaded (cached) - Syncing {title}..."));
                    } else {
                        app.messages.clear();
                        app.is_loading = true;
                        app.is_syncing = false;
                        app.set_status(format!("Loading {title}..."));
                    }

                    trigger_load_conversation(bg_tx.clone(), graph.clone(), conv, active_fetch_in_progress.clone());
                }
            }
            KeyCode::Char('r') => {
                app.set_status("Refreshing conversations in background...");
                trigger_refresh_sidebar(bg_tx.clone(), graph.clone());
            }
            _ => {}
        },

        FocusedPane::Messages => match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Tab => app.cycle_focus(),
            KeyCode::Esc => app.focused_pane = FocusedPane::Sidebar,
            KeyCode::Char('i') => app.focused_pane = FocusedPane::Input,
            KeyCode::Char('/' | 's') => app.open_search(),
            KeyCode::Char('k') | KeyCode::Up => app.scroll_messages_up(),
            KeyCode::Char('j') | KeyCode::Down => app.scroll_messages_down(),
            _ => {}
        },
    }
}

/// Helper: Trigger non-blocking background conversation message fetch
fn trigger_load_conversation(
    tx: mpsc::Sender<BackgroundEvent>,
    graph: Arc<GraphClient>,
    conv: ActiveConversation,
    guard: Arc<AtomicBool>,
) {
    guard.store(true, Ordering::Relaxed);
    tokio::spawn(async move {
        let t0 = Instant::now();
        let res = match &conv {
            ActiveConversation::Chat { id, .. } => graph.get_chat_messages(id).await,
            ActiveConversation::Channel { team_id, channel_id, .. } => {
                graph.get_channel_messages(team_id, channel_id).await
            }
        };
        guard.store(false, Ordering::Relaxed);
        let elapsed_ms = t0.elapsed().as_millis();
        match res {
            Ok(msgs) => {
                let _ = tx.send(BackgroundEvent::MessagesLoaded { conv, messages: msgs, elapsed_ms }).await;
            }
            Err(e) => {
                let _ = tx.send(BackgroundEvent::MessageLoadFailed { conv, error: e.to_string() }).await;
            }
        }
    });
}

/// Helper: Trigger non-blocking background sidebar refresh using `JoinSet` (no external futures crate)
fn trigger_refresh_sidebar(
    tx: mpsc::Sender<BackgroundEvent>,
    graph: Arc<GraphClient>,
) {
    tokio::spawn(async move {
        let t0 = Instant::now();
        let (chats_res, teams_res) = tokio::join!(
            graph.get_my_chats(),
            graph.get_joined_teams()
        );
        let chats = chats_res.unwrap_or_default();
        let teams = teams_res.unwrap_or_default();

        let mut join_set = JoinSet::new();
        for team in teams {
            let g = graph.clone();
            join_set.spawn(async move {
                let channels = g.get_team_channels(&team.id).await.unwrap_or_default();
                (team, channels)
            });
        }

        let mut teams_with_channels = Vec::new();
        while let Some(res) = join_set.join_next().await {
            if let Ok(item) = res {
                teams_with_channels.push(item);
            }
        }

        let elapsed_ms = t0.elapsed().as_millis();

        let _ = tx.send(BackgroundEvent::SidebarRefreshed {
            chats,
            teams: teams_with_channels,
            elapsed_ms,
        }).await;
    });
}

/// Helper: Trigger non-blocking background presence fetch for a list of users
fn trigger_load_presences(
    tx: mpsc::Sender<BackgroundEvent>,
    graph: Arc<GraphClient>,
    user_ids: Vec<String>,
) {
    if user_ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        let refs: Vec<&str> = user_ids.iter().map(std::string::String::as_str).collect();
        if let Ok(presences) = graph.get_users_presence(&refs).await {
            let _ = tx.send(BackgroundEvent::UserPresencesLoaded(presences)).await;
        }
    });
}
