use crossterm::event::{KeyCode, KeyEvent};

use crate::types::{PullRequest, ReviewRequest};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    MyPrs,
    ReviewRequests,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Loading,
    Ready,
    Error,
    Help,
}

pub struct App {
    pub tab: Tab,
    pub state: AppState,
    pub my_prs: Vec<PullRequest>,
    pub review_requests: Vec<ReviewRequest>,
    pub my_prs_index: usize,
    pub reviews_index: usize,
    pub error_message: String,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub tick: u64,
    pub sort_newest_first: bool,
    pub update_available: Option<String>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::MyPrs,
            state: AppState::Loading,
            my_prs: Vec::new(),
            review_requests: Vec::new(),
            my_prs_index: 0,
            reviews_index: 0,
            error_message: String::new(),
            should_quit: false,
            needs_refresh: false,
            tick: 0,
            sort_newest_first: true,
            update_available: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.state == AppState::Help {
            self.state = AppState::Ready;
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('r') => {
                if self.state == AppState::Ready || self.state == AppState::Error {
                    self.needs_refresh = true;
                    self.state = AppState::Loading;
                }
            }
            KeyCode::Char('?') => {
                if self.state == AppState::Ready {
                    self.state = AppState::Help;
                }
            }
            KeyCode::Tab => {
                self.tab = match self.tab {
                    Tab::MyPrs => Tab::ReviewRequests,
                    Tab::ReviewRequests => Tab::MyPrs,
                };
            }
            KeyCode::BackTab => {
                self.tab = match self.tab {
                    Tab::MyPrs => Tab::ReviewRequests,
                    Tab::ReviewRequests => Tab::MyPrs,
                };
            }
            KeyCode::Char('s') => {
                if self.state == AppState::Ready {
                    self.sort_newest_first = !self.sort_newest_first;
                    self.sort_lists();
                    self.clamp_indices();
                }
            }
            KeyCode::Char('1') => self.tab = Tab::MyPrs,
            KeyCode::Char('2') => self.tab = Tab::ReviewRequests,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Enter => self.open_selected(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let (index, len) = match self.tab {
            Tab::MyPrs => (&mut self.my_prs_index, self.my_prs.len()),
            Tab::ReviewRequests => (&mut self.reviews_index, self.review_requests.len()),
        };

        if len == 0 {
            return;
        }

        let new_index = (*index as i32 + delta).clamp(0, len as i32 - 1) as usize;
        *index = new_index;
    }

    fn open_selected(&self) {
        let url = match self.tab {
            Tab::MyPrs => self.my_prs.get(self.my_prs_index).map(|pr| &pr.url),
            Tab::ReviewRequests => self
                .review_requests
                .get(self.reviews_index)
                .map(|rr| &rr.url),
        };

        if let Some(url) = url {
            let _ = open::that(url);
        }
    }

    pub fn selected_index(&self) -> usize {
        match self.tab {
            Tab::MyPrs => self.my_prs_index,
            Tab::ReviewRequests => self.reviews_index,
        }
    }

    pub fn sort_lists(&mut self) {
        if self.sort_newest_first {
            self.my_prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            self.review_requests
                .sort_by(|a, b| b.requested_at.cmp(&a.requested_at));
        } else {
            self.my_prs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
            self.review_requests
                .sort_by(|a, b| a.requested_at.cmp(&b.requested_at));
        }
    }

    pub fn clamp_indices(&mut self) {
        if !self.my_prs.is_empty() {
            self.my_prs_index = self.my_prs_index.min(self.my_prs.len() - 1);
        } else {
            self.my_prs_index = 0;
        }
        if !self.review_requests.is_empty() {
            self.reviews_index = self.reviews_index.min(self.review_requests.len() - 1);
        } else {
            self.reviews_index = 0;
        }
    }
}
