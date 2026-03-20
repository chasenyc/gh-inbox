use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;

use crate::priority::{PriorityContext, compute_priority};
use crate::snake::SnakeGame;
use crate::types::{
    CiStatus, MergeStatus, Notification, PullRequest, ReviewRequest, ReviewStatus, WeeklyStats,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Inbox,
    MyPrs,
    ReviewRequests,
    Stats,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Loading,
    Ready,
    Error,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum SortOrder {
    NewestFirst,
    OldestFirst,
    PriorityFirst,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InboxFilter {
    All,
    HighAndCritical,
    CriticalOnly,
}

impl InboxFilter {
    pub fn next(self) -> Self {
        match self {
            InboxFilter::All => InboxFilter::HighAndCritical,
            InboxFilter::HighAndCritical => InboxFilter::CriticalOnly,
            InboxFilter::CriticalOnly => InboxFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InboxFilter::All => "",
            InboxFilter::HighAndCritical => " [High+]",
            InboxFilter::CriticalOnly => " [Critical]",
        }
    }
}

pub struct App {
    pub tab: Tab,
    pub state: AppState,
    pub my_prs: Vec<PullRequest>,
    pub review_requests: Vec<ReviewRequest>,
    pub notifications: Vec<Notification>,
    pub my_prs_table_state: TableState,
    pub reviews_table_state: TableState,
    pub inbox_table_state: TableState,
    pub error_message: String,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub tick: u64,
    pub sort_order: SortOrder,
    pub update_available: Option<String>,
    pub merged_stats: Option<WeeklyStats>,
    pub reviewed_stats: Option<WeeklyStats>,
    pub snake_game: Option<SnakeGame>,
    pub notifications_last_modified: Option<String>,
    pub inbox_filter: InboxFilter,
    pub notification_scope_missing: bool,
    pub pending_mark_read: Option<String>,
    pub pending_mark_all_read: bool,
    pub status_error: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::Inbox,
            state: AppState::Loading,
            my_prs: Vec::new(),
            review_requests: Vec::new(),
            notifications: Vec::new(),
            my_prs_table_state: TableState::default().with_selected(0),
            reviews_table_state: TableState::default().with_selected(0),
            inbox_table_state: TableState::default().with_selected(0),
            error_message: String::new(),
            should_quit: false,
            needs_refresh: false,
            tick: 0,
            sort_order: SortOrder::NewestFirst,
            update_available: None,
            merged_stats: None,
            reviewed_stats: None,
            snake_game: None,
            notifications_last_modified: None,
            inbox_filter: InboxFilter::All,
            notification_scope_missing: false,
            pending_mark_read: None,
            pending_mark_all_read: false,
            status_error: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Clear status error on any keypress
        self.status_error = None;

        if self.state == AppState::Help {
            self.state = AppState::Ready;
            return;
        }

        // Snake game input handling
        if let Some(ref mut game) = self.snake_game {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.snake_game = None;
                    return;
                }
                KeyCode::Char(' ') if game.game_over => {
                    let w = game.width;
                    let h = game.height;
                    self.snake_game = Some(SnakeGame::new(w, h));
                    return;
                }
                _ => {
                    game.handle_key(key.code);
                    return;
                }
            }
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('r') => {
                if self.state == AppState::Ready || self.state == AppState::Error {
                    self.needs_refresh = true;
                    self.state = AppState::Loading;
                    self.merged_stats = None;
                    self.reviewed_stats = None;
                }
            }
            KeyCode::Char('?') => {
                if self.state == AppState::Ready {
                    self.state = AppState::Help;
                }
            }
            KeyCode::Tab => {
                self.tab = match self.tab {
                    Tab::Inbox => Tab::MyPrs,
                    Tab::MyPrs => Tab::ReviewRequests,
                    Tab::ReviewRequests => Tab::Stats,
                    Tab::Stats => Tab::Inbox,
                };
            }
            KeyCode::BackTab => {
                self.tab = match self.tab {
                    Tab::Inbox => Tab::Stats,
                    Tab::MyPrs => Tab::Inbox,
                    Tab::ReviewRequests => Tab::MyPrs,
                    Tab::Stats => Tab::ReviewRequests,
                };
            }
            KeyCode::Char('s') => {
                if self.state == AppState::Ready {
                    self.sort_order = match self.sort_order {
                        SortOrder::NewestFirst => SortOrder::OldestFirst,
                        SortOrder::OldestFirst => SortOrder::PriorityFirst,
                        SortOrder::PriorityFirst => SortOrder::NewestFirst,
                    };
                    self.sort_lists();
                    self.clamp_indices();
                }
            }
            KeyCode::Char('1') => self.tab = Tab::Inbox,
            KeyCode::Char('2') => self.tab = Tab::MyPrs,
            KeyCode::Char('3') => self.tab = Tab::ReviewRequests,
            KeyCode::Char('4') => self.tab = Tab::Stats,
            KeyCode::Char('d') => {
                if self.state == AppState::Ready && self.tab == Tab::Inbox {
                    self.mark_selected_read();
                }
            }
            KeyCode::Char('D') => {
                if self.state == AppState::Ready && self.tab == Tab::Inbox {
                    self.mark_all_read();
                }
            }
            KeyCode::Char('f') => {
                if self.state == AppState::Ready && self.tab == Tab::Inbox {
                    self.inbox_filter = self.inbox_filter.next();
                    self.clamp_indices();
                }
            }
            KeyCode::Char(' ') => {
                if self.tab == Tab::MyPrs && self.my_prs.is_empty() && self.state == AppState::Ready {
                    self.snake_game = Some(SnakeGame::new(30, 15));
                }
            }
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Enter => self.open_selected(),
            _ => {}
        }
    }

    fn mark_selected_read(&mut self) {
        let filtered = self.filtered_notification_indices();
        let sel = self.inbox_table_state.selected().unwrap_or(0);
        if let Some(&real_index) = filtered.get(sel) {
            if let Some(notif) = self.notifications.get_mut(real_index) {
                if !notif.pending_read {
                    notif.pending_read = true;
                    self.pending_mark_read = Some(notif.id.clone());
                }
            }
        }
    }

    fn mark_all_read(&mut self) {
        for notif in &mut self.notifications {
            notif.pending_read = true;
        }
        self.pending_mark_all_read = true;
    }

    fn move_selection(&mut self, delta: i32) {
        let inbox_len = self.filtered_notification_indices().len();
        let (table_state, len) = match self.tab {
            Tab::Inbox => (&mut self.inbox_table_state, inbox_len),
            Tab::MyPrs => (&mut self.my_prs_table_state, self.my_prs.len()),
            Tab::ReviewRequests => (&mut self.reviews_table_state, self.review_requests.len()),
            Tab::Stats => return,
        };

        if len == 0 {
            return;
        }

        let current = table_state.selected().unwrap_or(0);
        let new_index = (current as i32 + delta).clamp(0, len as i32 - 1) as usize;
        table_state.select(Some(new_index));
    }

    fn open_selected(&self) {
        let url = match self.tab {
            Tab::Inbox => {
                let filtered = self.filtered_notification_indices();
                let sel = self.inbox_table_state.selected().unwrap_or(0);
                filtered.get(sel).and_then(|&i| {
                    self.notifications.get(i).map(|n| n.subject_url.clone())
                })
            }
            Tab::MyPrs => {
                let i = self.my_prs_table_state.selected().unwrap_or(0);
                self.my_prs.get(i).map(|pr| pr.url.clone())
            }
            Tab::ReviewRequests => {
                let i = self.reviews_table_state.selected().unwrap_or(0);
                self.review_requests.get(i).map(|rr| rr.url.clone())
            }
            Tab::Stats => None,
        };

        if let Some(url) = url {
            if !url.is_empty() {
                let _ = open::that(&url);
            }
        }
    }

    pub fn selected_index(&self) -> usize {
        match self.tab {
            Tab::Inbox => self.inbox_table_state.selected().unwrap_or(0),
            Tab::MyPrs => self.my_prs_table_state.selected().unwrap_or(0),
            Tab::ReviewRequests => self.reviews_table_state.selected().unwrap_or(0),
            Tab::Stats => 0,
        }
    }

    pub fn filtered_notification_indices(&self) -> Vec<usize> {
        use crate::types::Priority;
        self.notifications
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.pending_read)
            .filter(|(_, n)| match self.inbox_filter {
                InboxFilter::All => true,
                InboxFilter::HighAndCritical => {
                    n.priority == Priority::Critical || n.priority == Priority::High
                }
                InboxFilter::CriticalOnly => n.priority == Priority::Critical,
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn sort_lists(&mut self) {
        match self.sort_order {
            SortOrder::NewestFirst => {
                self.my_prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                self.review_requests.sort_by(|a, b| b.requested_at.cmp(&a.requested_at));
                self.notifications.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            }
            SortOrder::OldestFirst => {
                self.my_prs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
                self.review_requests.sort_by(|a, b| a.requested_at.cmp(&b.requested_at));
                self.notifications.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
            }
            SortOrder::PriorityFirst => {
                self.my_prs.sort_by(|a, b| {
                    b.priority_score.cmp(&a.priority_score)
                        .then(b.updated_at.cmp(&a.updated_at))
                });
                self.review_requests.sort_by(|a, b| {
                    b.priority_score.cmp(&a.priority_score)
                        .then(b.requested_at.cmp(&a.requested_at))
                });
                self.notifications.sort_by(|a, b| {
                    b.priority_score.cmp(&a.priority_score)
                        .then(b.updated_at.cmp(&a.updated_at))
                });
            }
        }
    }

    pub fn clamp_indices(&mut self) {
        let my_prs_sel = self.my_prs_table_state.selected().unwrap_or(0);
        if !self.my_prs.is_empty() {
            self.my_prs_table_state.select(Some(my_prs_sel.min(self.my_prs.len() - 1)));
        } else {
            self.my_prs_table_state.select(Some(0));
        }
        let reviews_sel = self.reviews_table_state.selected().unwrap_or(0);
        if !self.review_requests.is_empty() {
            self.reviews_table_state.select(Some(reviews_sel.min(self.review_requests.len() - 1)));
        } else {
            self.reviews_table_state.select(Some(0));
        }
        let filtered_len = self.filtered_notification_indices().len();
        let inbox_sel = self.inbox_table_state.selected().unwrap_or(0);
        if filtered_len > 0 {
            self.inbox_table_state.select(Some(inbox_sel.min(filtered_len - 1)));
        } else {
            self.inbox_table_state.select(Some(0));
        }
    }

    /// Recompute priority for a single PR by index.
    pub fn recompute_pr_priority(&mut self, index: usize) {
        if let Some(pr) = self.my_prs.get_mut(index) {
            let ctx = PriorityContext {
                ci_status: pr.ci_status.clone(),
                review_status: pr.review_status.clone(),
                merge_status: pr.merge_status.clone(),
                is_draft: pr.is_draft,
                is_stale: pr.is_stale(),
                is_direct_review_request: false,
                notification_reason: None,
            };
            let (p, s) = compute_priority(&ctx);
            pr.priority = p;
            pr.priority_score = s;
        }
    }

    /// Recompute priority for a single review request by index.
    pub fn recompute_review_priority(&mut self, index: usize) {
        if let Some(rr) = self.review_requests.get_mut(index) {
            let ctx = PriorityContext {
                ci_status: rr.ci_status.clone(),
                review_status: ReviewStatus::NoReviewers,
                merge_status: rr.merge_status.clone(),
                is_draft: rr.is_draft,
                is_stale: false,
                is_direct_review_request: rr.is_direct,
                notification_reason: None,
            };
            let (p, s) = compute_priority(&ctx);
            rr.priority = p;
            rr.priority_score = s;
        }
    }

    /// Compute priority for all notifications (called once after fetch).
    pub fn compute_notification_priorities(&mut self) {
        for notif in &mut self.notifications {
            let ctx = PriorityContext {
                ci_status: CiStatus::None,
                review_status: ReviewStatus::NoReviewers,
                merge_status: MergeStatus::Unknown,
                is_draft: false,
                is_stale: false,
                is_direct_review_request: false,
                notification_reason: Some(notif.reason.clone()),
            };
            let (p, s) = compute_priority(&ctx);
            notif.priority = p;
            notif.priority_score = s;
        }
    }
}
