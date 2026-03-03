mod app;
mod github;
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
    DirectRequestUpdate {
        index: usize,
        is_direct: bool,
    },
    UpdateAvailable(String),
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

    // Kick off initial fetch + version check
    spawn_data_fetch(tx.clone());
    spawn_update_check(tx.clone());

    loop {
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Check for incoming data messages (non-blocking)
        while let Ok(msg) = rx.try_recv() {
            match msg {
                DataMsg::InitialData {
                    my_prs,
                    review_requests,
                } => {
                    app.my_prs = my_prs;
                    app.review_requests = review_requests;
                    app.sort_lists();
                    app.state = app::AppState::Ready;
                    app.clamp_indices();
                }
                DataMsg::CiUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.ci_status = status;
                    }
                }
                DataMsg::ReviewUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.review_status = status;
                    }
                }
                DataMsg::MergeStatusUpdate { index, status } => {
                    if let Some(pr) = app.my_prs.get_mut(index) {
                        pr.merge_status = status;
                    }
                }
                DataMsg::DirectRequestUpdate { index, is_direct } => {
                    if let Some(rr) = app.review_requests.get_mut(index) {
                        rr.is_direct = is_direct;
                    }
                }
                DataMsg::UpdateAvailable(version) => {
                    app.update_available = Some(version);
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
            spawn_data_fetch(tx.clone());
        }

        // Poll for keyboard events with a short timeout so we can process data messages
        if event::poll(std::time::Duration::from_millis(50))? {
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

fn spawn_data_fetch(tx: mpsc::Sender<DataMsg>) {
    tokio::spawn(async move {
        let client = match GitHubClient::new() {
            Ok(c) => Arc::new(c),
            Err(e) => {
                let _ = tx.send(DataMsg::Error(e.to_string())).await;
                return;
            }
        };

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

        // Review request direct/team detection
        for i in 0..rr_count {
            let client = Arc::clone(&client);
            let tx = tx.clone();
            let sem = Arc::clone(&semaphore);
            let (repo, url) = rrs_for_direct[i].clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;

                let is_direct = client.fetch_is_direct_request(&repo, &url).await;
                let _ = tx
                    .send(DataMsg::DirectRequestUpdate { index: i, is_direct })
                    .await;
            }));
        }

        for h in handles {
            let _ = h.await;
        }
    });
}

fn spawn_update_check(tx: mpsc::Sender<DataMsg>) {
    tokio::spawn(async move {
        if let Some(version) = github::check_for_update().await {
            let _ = tx.send(DataMsg::UpdateAvailable(version)).await;
        }
    });
}
