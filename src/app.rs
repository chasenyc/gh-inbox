use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;

use crate::priority::{PriorityContext, compute_priority};
use crate::snake::SnakeGame;
use crate::types::{
    CiStatus, MergeStatus, Notification, PullRequest, ReviewRequest, ReviewStatus, WeeklyStats,
};

#[derive(Debug, Clone, PartialEq)]
pub enum StackPosition {
    Standalone,
    StackBase,
    StackMiddle,
    StackTop,
}

#[derive(Debug, Clone)]
pub struct StackEntry {
    pub original_index: usize,
    pub stack_position: StackPosition,
}

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
    Smart,
    All,
}

impl InboxFilter {
    pub fn next(self) -> Self {
        match self {
            InboxFilter::Smart => InboxFilter::All,
            InboxFilter::All => InboxFilter::Smart,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            InboxFilter::Smart => "",
            InboxFilter::All => " [All]",
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
    pub my_prs_display_order: Vec<StackEntry>,
    pub reviews_display_order: Vec<StackEntry>,
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
            inbox_filter: InboxFilter::Smart,
            notification_scope_missing: false,
            pending_mark_read: None,
            pending_mark_all_read: false,
            status_error: None,
            my_prs_display_order: Vec::new(),
            reviews_display_order: Vec::new(),
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
            Tab::MyPrs => (&mut self.my_prs_table_state, self.my_prs_display_order.len()),
            Tab::ReviewRequests => (&mut self.reviews_table_state, self.reviews_display_order.len()),
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
                self.my_prs_display_order.get(i)
                    .and_then(|e| self.my_prs.get(e.original_index))
                    .map(|pr| pr.url.clone())
            }
            Tab::ReviewRequests => {
                let i = self.reviews_table_state.selected().unwrap_or(0);
                self.reviews_display_order.get(i)
                    .and_then(|e| self.review_requests.get(e.original_index))
                    .map(|rr| rr.url.clone())
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
        use crate::types::{NotificationReason, SubjectState};
        self.notifications
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.pending_read)
            .filter(|(_, n)| match self.inbox_filter {
                InboxFilter::Smart => {
                    // Hide CI activity and state changes (noise, never actionable)
                    if matches!(
                        n.reason,
                        NotificationReason::CiActivity | NotificationReason::StateChange
                    ) {
                        return false;
                    }
                    // Only show once we've confirmed the subject is open
                    n.subject_state == SubjectState::Open
                }
                InboxFilter::All => true,
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
        self.recompute_display_order();
    }

    pub fn clamp_indices(&mut self) {
        let my_prs_len = self.my_prs_display_order.len().max(self.my_prs.len());
        let my_prs_sel = self.my_prs_table_state.selected().unwrap_or(0);
        if my_prs_len > 0 {
            self.my_prs_table_state.select(Some(my_prs_sel.min(my_prs_len - 1)));
        } else {
            self.my_prs_table_state.select(Some(0));
        }
        let reviews_len = self.reviews_display_order.len().max(self.review_requests.len());
        let reviews_sel = self.reviews_table_state.selected().unwrap_or(0);
        if reviews_len > 0 {
            self.reviews_table_state.select(Some(reviews_sel.min(reviews_len - 1)));
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

    /// Recompute display order for My PRs and Review Requests, grouping stacked PRs.
    pub fn recompute_display_order(&mut self) {
        self.my_prs_display_order = compute_stack_order(
            self.my_prs.iter().map(|pr| (pr.repo.as_str(), pr.head_ref.as_deref(), pr.base_ref.as_deref())).collect(),
        );
        self.reviews_display_order = compute_stack_order(
            self.review_requests.iter().map(|rr| (rr.repo.as_str(), rr.head_ref.as_deref(), rr.base_ref.as_deref())).collect(),
        );
    }
}

/// Given a list of (repo, head_ref, base_ref) tuples indexed by position,
/// compute a display order that groups stacked PRs together (base first).
fn compute_stack_order(items: Vec<(&str, Option<&str>, Option<&str>)>) -> Vec<StackEntry> {
    if items.is_empty() {
        return Vec::new();
    }

    // Group indices by repo
    let mut by_repo: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, (repo, _, _)) in items.iter().enumerate() {
        by_repo.entry(repo).or_default().push(i);
    }

    // Track which indices are part of a stack
    let mut in_stack: Vec<bool> = vec![false; items.len()];
    // Collect ordered stacks: each stack is a Vec of indices from base to tip
    let mut stacks: Vec<Vec<usize>> = Vec::new();

    for (_repo, indices) in &by_repo {
        if indices.len() < 2 {
            continue;
        }

        // Build a map: head_ref -> index (for PRs that have branch info)
        let mut head_to_idx: HashMap<&str, usize> = HashMap::new();
        for &i in indices {
            if let Some(head) = items[i].1 {
                head_to_idx.insert(head, i);
            }
        }

        // Find chains: a PR is a child if its base_ref matches another PR's head_ref
        // parent[i] = Some(j) means PR i's base_ref == PR j's head_ref
        let mut parent: HashMap<usize, usize> = HashMap::new();
        let mut children: HashMap<usize, usize> = HashMap::new();
        for &i in indices {
            if let Some(base) = items[i].2 {
                if let Some(&parent_idx) = head_to_idx.get(base) {
                    if parent_idx != i {
                        parent.insert(i, parent_idx);
                        children.insert(parent_idx, i);
                    }
                }
            }
        }

        // Find roots: PRs that are parents but not children themselves
        let roots: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|i| children.contains_key(i) || parent.contains_key(i))
            .filter(|i| !parent.contains_key(i))
            .collect();

        for root in roots {
            let mut chain = vec![root];
            let mut current = root;
            // Walk from root to tip
            while let Some(&child) = children.get(&current) {
                chain.push(child);
                current = child;
            }
            if chain.len() >= 2 {
                for &idx in &chain {
                    in_stack[idx] = true;
                }
                stacks.push(chain);
            }
        }
    }

    // Build final display order: preserve original ordering for standalone items,
    // insert stacks at the position of their first (base) member
    let mut result: Vec<StackEntry> = Vec::with_capacity(items.len());
    let mut stack_inserted: Vec<bool> = vec![false; stacks.len()];

    // Map each stacked index to its stack number for insertion
    let mut idx_to_stack: HashMap<usize, usize> = HashMap::new();
    for (si, stack) in stacks.iter().enumerate() {
        for &idx in stack {
            idx_to_stack.insert(idx, si);
        }
    }

    for i in 0..items.len() {
        if in_stack[i] {
            if let Some(&si) = idx_to_stack.get(&i) {
                if !stack_inserted[si] {
                    stack_inserted[si] = true;
                    let chain = &stacks[si];
                    let last = chain.len() - 1;
                    for (pos, &idx) in chain.iter().enumerate() {
                        let stack_position = if pos == 0 && last == 0 {
                            StackPosition::Standalone
                        } else if pos == 0 {
                            StackPosition::StackBase
                        } else if pos == last {
                            StackPosition::StackTop
                        } else {
                            StackPosition::StackMiddle
                        };
                        result.push(StackEntry {
                            original_index: idx,
                            stack_position,
                        });
                    }
                }
            }
        } else {
            result.push(StackEntry {
                original_index: i,
                stack_position: StackPosition::Standalone,
            });
        }
    }

    result
}
