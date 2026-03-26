mod app;
mod github;
mod priority;
mod snake;
mod types;
mod ui;

use std::io;
use std::sync::Arc;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use app::App;
use github::GitHubClient;

enum DataMsg {
    InitialData {
        my_prs: Vec<types::PullRequest>,
        review_requests: Vec<types::ReviewRequest>,
    },
    CiUpdate {
        index: usize,
        status: types::CiStatus,
    },
    ReviewUpdate {
        index: usize,
        status: types::ReviewStatus,
    },
    MergeStatusUpdate {
        index: usize,
        status: types::MergeStatus,
    },
    ReviewCiUpdate {
        index: usize,
        status: types::CiStatus,
    },
    DirectRequestUpdate {
        index: usize,
        is_direct: bool,
    },
    ReviewMergeStatusUpdate {
        index: usize,
        status: types::MergeStatus,
    },
    StatsData {
        merged: types::WeeklyStats,
        reviewed: types::WeeklyStats,
    },
    UpdateAvailable(String),
    NotificationsData {
        notifications: Vec<types::Notification>,
        last_modified: Option<String>,
    },
    NotificationScopeMissing,
    NotificationReadOk {
        id: String,
    },
    NotificationReadErr {
        id: String,
        error: String,
    },
    NotificationAllReadOk,
    NotificationAllReadErr {
        error: String,
    },
    NotificationSubjectState {
        notification_id: String,
        state: types::SubjectState,
        is_draft: bool,
        author: String,
        merge_status: types::MergeStatus,
    },
    Error(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Handle --help and --version
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("gh-inbox — GitHub PR Dashboard TUI");
        println!();
        println!("Usage: gh-inbox");
        println!();
        println!("Requires: GitHub CLI (`gh`) authenticated via `gh auth login`");
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("gh-inbox {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Init terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::channel::<DataMsg>(64);

    // Create client once and share via Arc (decision 6A from eng review)
    let client = match GitHubClient::new().await {
        Ok(c) => {
            let client = Arc::new(c);
            spawn_data_fetch(client.clone(), tx.clone());
            spawn_notification_fetch(client.clone(), None, tx.clone());
            spawn_stats_fetch(client.clone(), tx.clone());
            spawn_update_check(tx.clone());
            Some(client)
        }
        Err(e) => {
            app.error_message = e.to_string();
            app.state = app::AppState::Error;
            None
        }
    };

    loop {
        app.tick = app.tick.wrapping_add(1);
        if let Some(ref mut game) = app.snake_game {
            game.tick();
        }
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        // Check for incoming data messages (non-blocking)
        while let Ok(msg) = rx.try_recv() {
            match msg {
                DataMsg::InitialData {
                    my_prs,
                    review_requests,
                } => {
                    app.my_prs = my_prs;
                    app.review_requests = review_requests;
                    app.sort_order = app::SortOrder::NewestFirst;
                    app.state = app::AppState::Ready;
                    app.clamp_indices();
                }
                DataMsg::CiUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.ci_status = status;
                    }
                    app.recompute_pr_priority(index);
                }
                DataMsg::ReviewUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.review_status = status;
                    }
                    app.recompute_pr_priority(index);
                }
                DataMsg::MergeStatusUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.merge_status = status;
                    }
                    app.recompute_pr_priority(index);
                }
                DataMsg::ReviewCiUpdate { index, status } => {
                    if let Some(rr) = app.review_requests.get_mut(index) {
                        rr.ci_status = status;
                    }
                    app.recompute_review_priority(index);
                }
                DataMsg::DirectRequestUpdate { index, is_direct } => {
                    if let Some(rr) = app.review_requests.get_mut(index) {
                        rr.is_direct = is_direct;
                    }
                    app.recompute_review_priority(index);
                }
                DataMsg::ReviewMergeStatusUpdate { index, status } => {
                    if let Some(rr) = app.review_requests.get_mut(index) {
                        rr.merge_status = status;
                    }
                    app.recompute_review_priority(index);
                }
                DataMsg::StatsData { merged, reviewed } => {
                    app.merged_stats = Some(merged);
                    app.reviewed_stats = Some(reviewed);
                }
                DataMsg::UpdateAvailable(version) => {
                    app.update_available = Some(version);
                }
                DataMsg::NotificationsData {
                    notifications,
                    last_modified,
                } => {
                    app.notifications = notifications;
                    app.notifications_last_modified = last_modified;
                    app.compute_notification_priorities();
                    // Default sort for inbox is priority-first
                    app.notifications.sort_by(|a, b| {
                        b.priority_score
                            .cmp(&a.priority_score)
                            .then(b.updated_at.cmp(&a.updated_at))
                    });
                    app.clamp_indices();
                    if app.state == app::AppState::Loading {
                        app.state = app::AppState::Ready;
                    }
                    // Enrich subject state for notifications that pass reason filter
                    if let Some(ref client) = client {
                        let to_enrich: Vec<(String, String)> = app
                            .notifications
                            .iter()
                            .filter(|n| {
                                !matches!(
                                    n.reason,
                                    types::NotificationReason::CiActivity
                                        | types::NotificationReason::StateChange
                                )
                            })
                            .filter_map(|n| {
                                n.subject_api_url
                                    .as_ref()
                                    .map(|url| (n.id.clone(), url.clone()))
                            })
                            .collect();
                        spawn_notification_enrichment(client.clone(), to_enrich, tx.clone());
                    }
                }
                DataMsg::NotificationScopeMissing => {
                    app.notification_scope_missing = true;
                    if app.state == app::AppState::Loading {
                        app.state = app::AppState::Ready;
                    }
                }
                DataMsg::NotificationReadOk { id } => {
                    app.notifications.retain(|n| n.id != id);
                    app.clamp_indices();
                }
                DataMsg::NotificationReadErr { id, error } => {
                    // Restore: unset pending_read
                    if let Some(notif) = app.notifications.iter_mut().find(|n| n.id == id) {
                        notif.pending_read = false;
                    }
                    app.status_error = Some(format!("Mark read failed: {}", error));
                }
                DataMsg::NotificationAllReadOk => {
                    app.notifications.clear();
                    app.clamp_indices();
                }
                DataMsg::NotificationAllReadErr { error } => {
                    for notif in &mut app.notifications {
                        notif.pending_read = false;
                    }
                    app.status_error = Some(format!("Mark all read failed: {}", error));
                }
                DataMsg::NotificationSubjectState {
                    notification_id,
                    state,
                    is_draft,
                    author,
                    merge_status,
                } => {
                    if let Some(notif) = app
                        .notifications
                        .iter_mut()
                        .find(|n| n.id == notification_id)
                    {
                        notif.subject_state = state;
                        notif.is_draft = is_draft;
                        notif.author = author;
                        notif.merge_status = merge_status;
                    }
                    app.clamp_indices();
                }
                DataMsg::Error(msg) => {
                    app.error_message = msg;
                    app.state = app::AppState::Error;
                }
            }
        }

        // Handle refresh request
        if app.needs_refresh {
            app.needs_refresh = false;
            if let Some(ref client) = client {
                spawn_data_fetch(client.clone(), tx.clone());
                spawn_notification_fetch(
                    client.clone(),
                    app.notifications_last_modified.clone(),
                    tx.clone(),
                );
                spawn_stats_fetch(client.clone(), tx.clone());
            }
        }

        // Handle pending mark-as-read actions
        if let Some(notif_id) = app.pending_mark_read.take() {
            if let Some(ref client) = client {
                spawn_mark_notification_read(client.clone(), notif_id, tx.clone());
            }
        }
        if app.pending_mark_all_read {
            app.pending_mark_all_read = false;
            if let Some(ref client) = client {
                spawn_mark_all_notifications_read(client.clone(), tx.clone());
            }
        }

        // Poll for keyboard events with a short timeout so we can process data messages
        let poll_duration = if app.snake_game.is_some() {
            std::time::Duration::from_millis(16) // ~60fps during snake
        } else {
            std::time::Duration::from_millis(50)
        };
        if event::poll(poll_duration)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn spawn_data_fetch(client: Arc<GitHubClient>, tx: mpsc::Sender<DataMsg>) {
    tokio::spawn(async move {
        // Fetch both lists concurrently
        let (prs_result, reviews_result) = tokio::join!(
            client.fetch_my_prs(),
            client.fetch_review_requests()
        );

        let my_prs = match prs_result {
            Ok(prs) => prs,
            Err(e) => {
                let _ = tx
                    .send(DataMsg::Error(format!("Failed to fetch PRs: {}", e)))
                    .await;
                return;
            }
        };

        let review_requests = match reviews_result {
            Ok(rr) => rr,
            Err(e) => {
                let _ = tx
                    .send(DataMsg::Error(format!(
                        "Failed to fetch review requests: {}",
                        e
                    )))
                    .await;
                return;
            }
        };

        // Collect info for background enrichment before sending initial data
        let pr_count = my_prs.len();
        let prs_for_status: Vec<(String, String)> = my_prs
            .iter()
            .map(|pr| (pr.repo.clone(), pr.url.clone()))
            .collect();

        let rr_count = review_requests.len();
        let rrs_for_direct: Vec<(String, String)> = review_requests
            .iter()
            .map(|rr| (rr.repo.clone(), rr.url.clone()))
            .collect();

        // Send initial data immediately so the UI is responsive
        let _ = tx
            .send(DataMsg::InitialData {
                my_prs,
                review_requests,
            })
            .await;

        // Background enrichment (bounded concurrency)
        let semaphore = Arc::new(tokio::sync::Semaphore::new(8));

        let mut handles = Vec::new();

        // PR CI + review status
        for i in 0..pr_count {
            let client = Arc::clone(&client);
            let tx = tx.clone();
            let sem = Arc::clone(&semaphore);
            let (repo, url) = prs_for_status[i].clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;

                let (ci, (review, merge)) = tokio::join!(
                    client.fetch_ci_status(&repo, &url),
                    client.fetch_review_and_merge_status(&repo, &url),
                );

                let _ = tx.send(DataMsg::CiUpdate { index: i, status: ci }).await;
                let _ = tx
                    .send(DataMsg::ReviewUpdate {
                        index: i,
                        status: review,
                    })
                    .await;
                let _ = tx
                    .send(DataMsg::MergeStatusUpdate {
                        index: i,
                        status: merge,
                    })
                    .await;
            }));
        }

        // Review request direct/team detection + CI status
        for i in 0..rr_count {
            let client = Arc::clone(&client);
            let tx = tx.clone();
            let sem = Arc::clone(&semaphore);
            let (repo, url) = rrs_for_direct[i].clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;

                let ((is_direct, merge), ci) = tokio::join!(
                    client.fetch_is_direct_request(&repo, &url),
                    client.fetch_ci_status(&repo, &url),
                );
                let _ = tx
                    .send(DataMsg::DirectRequestUpdate { index: i, is_direct })
                    .await;
                let _ = tx
                    .send(DataMsg::ReviewCiUpdate { index: i, status: ci })
                    .await;
                let _ = tx
                    .send(DataMsg::ReviewMergeStatusUpdate { index: i, status: merge })
                    .await;
            }));
        }

        for h in handles {
            let _ = h.await;
        }
    });
}

fn spawn_notification_fetch(
    client: Arc<GitHubClient>,
    last_modified: Option<String>,
    tx: mpsc::Sender<DataMsg>,
) {
    tokio::spawn(async move {
        if !client.has_notifications_scope {
            let _ = tx.send(DataMsg::NotificationScopeMissing).await;
            return;
        }

        match client
            .fetch_notifications(true, last_modified.as_deref())
            .await
        {
            Ok(Some((notifications, new_last_modified))) => {
                let _ = tx
                    .send(DataMsg::NotificationsData {
                        notifications,
                        last_modified: new_last_modified,
                    })
                    .await;
            }
            Ok(None) => {
                // 304 Not Modified — keep existing list
            }
            Err(e) => {
                // Non-fatal: notification fetch failure shouldn't block the app
                let _ = tx
                    .send(DataMsg::NotificationScopeMissing)
                    .await;
                let _ = e; // suppress unused warning
            }
        }
    });
}

fn spawn_mark_notification_read(
    client: Arc<GitHubClient>,
    thread_id: String,
    tx: mpsc::Sender<DataMsg>,
) {
    tokio::spawn(async move {
        match client.mark_notification_read(&thread_id).await {
            Ok(()) => {
                let _ = tx
                    .send(DataMsg::NotificationReadOk { id: thread_id })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(DataMsg::NotificationReadErr {
                        id: thread_id,
                        error: e.to_string(),
                    })
                    .await;
            }
        }
    });
}

fn spawn_mark_all_notifications_read(
    client: Arc<GitHubClient>,
    tx: mpsc::Sender<DataMsg>,
) {
    tokio::spawn(async move {
        match client.mark_all_notifications_read().await {
            Ok(()) => {
                let _ = tx.send(DataMsg::NotificationAllReadOk).await;
            }
            Err(e) => {
                let _ = tx
                    .send(DataMsg::NotificationAllReadErr {
                        error: e.to_string(),
                    })
                    .await;
            }
        }
    });
}

fn spawn_notification_enrichment(
    client: Arc<GitHubClient>,
    notifications: Vec<(String, String)>, // (id, subject_api_url)
    tx: mpsc::Sender<DataMsg>,
) {
    tokio::spawn(async move {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
        let mut handles = Vec::new();

        for (notification_id, api_url) in notifications {
            let client = Arc::clone(&client);
            let tx = tx.clone();
            let sem = Arc::clone(&semaphore);

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let (state, is_draft, author, merge_status) =
                    client.fetch_subject_state(&api_url).await;
                let _ = tx
                    .send(DataMsg::NotificationSubjectState {
                        notification_id,
                        state,
                        is_draft,
                        author,
                        merge_status,
                    })
                    .await;
            }));
        }

        for h in handles {
            let _ = h.await;
        }
    });
}

fn spawn_stats_fetch(client: Arc<GitHubClient>, tx: mpsc::Sender<DataMsg>) {
    tokio::spawn(async move {
        let (merged, reviewed) = tokio::join!(
            client.fetch_merged_prs_stats(12),
            client.fetch_reviewed_prs_stats(12),
        );
        let _ = tx
            .send(DataMsg::StatsData { merged, reviewed })
            .await;
    });
}

fn spawn_update_check(tx: mpsc::Sender<DataMsg>) {
    tokio::spawn(async move {
        if let Some(version) = github::check_for_update().await {
            let _ = tx.send(DataMsg::UpdateAvailable(version)).await;
        }
    });
}
