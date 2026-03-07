use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;

use crate::snake::SnakeGame;
use crate::types::{PullRequest, ReviewRequest, WeeklyStats};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
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

pub struct App {
    pub tab: Tab,
    pub state: AppState,
    pub my_prs: Vec<PullRequest>,
    pub review_requests: Vec<ReviewRequest>,
    pub my_prs_table_state: TableState,
    pub reviews_table_state: TableState,
    pub error_message: String,
    pub should_quit: bool,
    pub needs_refresh: bool,
    pub tick: u64,
    pub sort_newest_first: bool,
    pub update_available: Option<String>,
    pub merged_stats: Option<WeeklyStats>,
    pub reviewed_stats: Option<WeeklyStats>,
    pub snake_game: Option<SnakeGame>,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: Tab::MyPrs,
            state: AppState::Loading,
            my_prs: Vec::new(),
            review_requests: Vec::new(),
            my_prs_table_state: TableState::default().with_selected(0),
            reviews_table_state: TableState::default().with_selected(0),
            error_message: String::new(),
            should_quit: false,
            needs_refresh: false,
            tick: 0,
            sort_newest_first: true,
            update_available: None,
            merged_stats: None,
            reviewed_stats: None,
            snake_game: None,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
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
                    Tab::MyPrs => Tab::ReviewRequests,
                    Tab::ReviewRequests => Tab::Stats,
                    Tab::Stats => Tab::MyPrs,
                };
            }
            KeyCode::BackTab => {
                self.tab = match self.tab {
                    Tab::MyPrs => Tab::Stats,
                    Tab::ReviewRequests => Tab::MyPrs,
                    Tab::Stats => Tab::ReviewRequests,
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
            KeyCode::Char('3') => self.tab = Tab::Stats,
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

    fn move_selection(&mut self, delta: i32) {
        let (table_state, len) = match self.tab {
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
            Tab::MyPrs => {
                let i = self.my_prs_table_state.selected().unwrap_or(0);
                self.my_prs.get(i).map(|pr| &pr.url)
            }
            Tab::ReviewRequests => {
                let i = self.reviews_table_state.selected().unwrap_or(0);
                self.review_requests.get(i).map(|rr| &rr.url)
            }
            Tab::Stats => None,
        };

        if let Some(url) = url {
            let _ = open::that(url);
        }
    }

    pub fn selected_index(&self) -> usize {
        match self.tab {
            Tab::MyPrs => self.my_prs_table_state.selected().unwrap_or(0),
            Tab::ReviewRequests => self.reviews_table_state.selected().unwrap_or(0),
            Tab::Stats => 0,
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
    }
}
