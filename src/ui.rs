use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row, Table},
};

use crate::app::{App, AppState, Tab};
use crate::types::{CiStatus, MergeStatus, ReviewStatus};

// ── Color Palette (Catppuccin Mocha-inspired) ───────────────────────

const TEXT: Color = Color::Rgb(205, 214, 244);
const SUBTEXT: Color = Color::Rgb(166, 173, 200);
const OVERLAY_TEXT: Color = Color::Rgb(108, 112, 134);
const SURFACE: Color = Color::Rgb(30, 30, 46);
const BORDER: Color = Color::Rgb(69, 71, 90);
const BLUE: Color = Color::Rgb(137, 180, 250);
const LAVENDER: Color = Color::Rgb(180, 190, 254);
const GREEN: Color = Color::Rgb(166, 218, 149);
const RED: Color = Color::Rgb(243, 139, 168);
const YELLOW: Color = Color::Rgb(249, 226, 175);
const PEACH: Color = Color::Rgb(250, 179, 135);
const AMBER: Color = Color::Rgb(245, 186, 100);
const SELECTED_BG: Color = Color::Rgb(40, 42, 58);

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// Hand-picked repo colors — distinct, readable on dark backgrounds,
// and avoids colliding with status colors (green/red/yellow).
const REPO_COLORS: &[Color] = &[
    Color::Rgb(137, 180, 250), // blue
    Color::Rgb(203, 166, 247), // mauve
    Color::Rgb(148, 226, 213), // teal
    Color::Rgb(245, 194, 231), // pink
    Color::Rgb(180, 190, 254), // lavender
    Color::Rgb(116, 199, 236), // sapphire
    Color::Rgb(250, 179, 135), // peach
    Color::Rgb(242, 205, 205), // flamingo
    Color::Rgb(137, 220, 235), // sky
    Color::Rgb(245, 224, 220), // rosewater
];

fn repo_color(repo: &str) -> Color {
    // Simple djb2-style hash — deterministic, no storage needed
    let hash = repo.bytes().fold(5381u64, |h, b| h.wrapping_mul(33).wrapping_add(b as u64));
    REPO_COLORS[(hash % REPO_COLORS.len() as u64) as usize]
}

// ── Main draw ───────────────────────────────────────────────────────

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Outer border wrapping the entire app
    let title = Line::from(vec![
        Span::styled(" ◆ ", Style::default().fg(BLUE)),
        Span::styled("gh-inbox ", Style::default().fg(LAVENDER).bold()),
    ]);

    let footer = footer_hints();

    let mut outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(title)
        .title_bottom(footer);

    if let Some(version) = &app.update_available {
        let update_line = Line::from(vec![
            Span::styled("⚠ v", Style::default().fg(AMBER).bold()),
            Span::styled(version.clone(), Style::default().fg(AMBER).bold()),
            Span::styled(" available · ", Style::default().fg(AMBER).bold()),
            Span::styled("brew upgrade gh-inbox ", Style::default().fg(AMBER)),
        ])
        .right_aligned();
        outer_block = outer_block.title_bottom(update_line);
    }

    let inner = outer_block.inner(area);
    frame.render_widget(outer_block, area);

    // Inner layout: tabs + content
    let chunks = Layout::vertical([
        Constraint::Length(2), // tab bar (line + spacing)
        Constraint::Min(3),   // content area
    ])
    .split(inner);

    let tab_area = inset_horizontal(chunks[0], 1);
    draw_tabs(frame, tab_area, app);

    let content_area = chunks[1];

    match app.state {
        AppState::Loading => draw_loading(frame, content_area, app.tick),
        AppState::Error => draw_error(frame, content_area, &app.error_message),
        AppState::Ready | AppState::Help => {
            draw_content(frame, content_area, app);
            if app.state == AppState::Help {
                draw_help_overlay(frame, area);
            }
        }
    }
}

// ── Footer keybinding hints (rendered inside bottom border) ─────────

fn footer_hints() -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" navigate ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("⏎", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" open ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("⇥", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" switch ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("r", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" refresh ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("s", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" sort ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("?", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" help ", Style::default().fg(OVERLAY_TEXT)),
        Span::styled("q", Style::default().fg(SUBTEXT).bold()),
        Span::styled(" quit ", Style::default().fg(OVERLAY_TEXT)),
    ])
}

// ── Tab bar ─────────────────────────────────────────────────────────

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let (my_dot, my_label, my_count_s) =
        tab_spans("My PRs", app.my_prs.len(), app.tab == Tab::MyPrs);
    let (rev_dot, rev_label, rev_count_s) =
        tab_spans("Reviews", app.review_requests.len(), app.tab == Tab::ReviewRequests);

    let line = Line::from(vec![
        Span::raw(" "),
        my_dot,
        my_label,
        my_count_s,
        Span::raw("       "),
        rev_dot,
        rev_label,
        rev_count_s,
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn tab_spans<'a>(label: &'a str, count: usize, active: bool) -> (Span<'a>, Span<'a>, Span<'a>) {
    if active {
        (
            Span::styled("● ", Style::default().fg(BLUE)),
            Span::styled(label, Style::default().fg(TEXT).bold()),
            Span::styled(format!("  {}", count), Style::default().fg(BLUE)),
        )
    } else {
        (
            Span::styled("○ ", Style::default().fg(OVERLAY_TEXT)),
            Span::styled(label, Style::default().fg(OVERLAY_TEXT)),
            Span::styled(
                format!("  {}", count),
                Style::default().fg(OVERLAY_TEXT),
            ),
        )
    }
}

// ── Content dispatch ────────────────────────────────────────────────

fn draw_content(frame: &mut Frame, area: Rect, app: &App) {
    match app.tab {
        Tab::MyPrs => draw_my_prs_table(frame, area, app),
        Tab::ReviewRequests => draw_reviews_table(frame, area, app),
    }
}

// ── My PRs table ────────────────────────────────────────────────────

fn draw_my_prs_table(frame: &mut Frame, area: Rect, app: &App) {
    if app.my_prs.is_empty() {
        draw_centered_message(frame, area, "✓  No open PRs — nice work!", OVERLAY_TEXT);
        return;
    }

    let header = Row::new(vec![
        Cell::from("   REPO"),
        Cell::from("TITLE"),
        Cell::from("CI"),
        Cell::from("REVIEW"),
        Cell::from("MERGE"),
        Cell::from("AGE"),
    ])
    .style(Style::default().fg(OVERLAY_TEXT).bold())
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .my_prs
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let selected = i == app.selected_index();

            // Accent bar + repo (color derived from repo name)
            let rc = repo_color(&pr.repo);
            let (accent, repo_style, title_style) = if selected {
                (
                    Span::styled(" ▎ ", Style::default().fg(BLUE)),
                    if pr.is_draft {
                        Style::default().fg(OVERLAY_TEXT).italic()
                    } else {
                        Style::default().fg(rc).bold()
                    },
                    if pr.is_draft {
                        Style::default().fg(OVERLAY_TEXT).italic()
                    } else {
                        Style::default().fg(TEXT).bold()
                    },
                )
            } else {
                (
                    Span::styled("   ", Style::default()),
                    if pr.is_draft {
                        Style::default().fg(OVERLAY_TEXT).italic()
                    } else {
                        Style::default().fg(rc)
                    },
                    if pr.is_draft {
                        Style::default().fg(OVERLAY_TEXT).italic()
                    } else {
                        Style::default().fg(TEXT)
                    },
                )
            };

            let repo_cell = Cell::from(Line::from(vec![
                accent,
                Span::styled(pr.repo.clone(), repo_style),
            ]));

            // CI status
            let (ci_sym, ci_color) = match pr.ci_status {
                CiStatus::Passing => ("✓", GREEN),
                CiStatus::Failing => ("✗", RED),
                CiStatus::Pending => ("◌", YELLOW),
                CiStatus::None => ("·", OVERLAY_TEXT),
            };

            // Review status
            let review_cell = match pr.review_status {
                ReviewStatus::Approved => Cell::from(Line::from(vec![
                    Span::styled("● ", Style::default().fg(GREEN)),
                    Span::styled("Approved", Style::default().fg(GREEN)),
                ])),
                ReviewStatus::ChangesRequested => Cell::from(Line::from(vec![
                    Span::styled("● ", Style::default().fg(PEACH)),
                    Span::styled("Changes", Style::default().fg(PEACH)),
                ])),
                ReviewStatus::Pending => Cell::from(Line::from(vec![
                    Span::styled("● ", Style::default().fg(YELLOW)),
                    Span::styled("Pending", Style::default().fg(YELLOW)),
                ])),
                ReviewStatus::NoReviewers => {
                    Cell::from(Span::styled("—", Style::default().fg(OVERLAY_TEXT)))
                }
            };

            // Merge status
            let (merge_sym, merge_color) = match pr.merge_status {
                MergeStatus::Ready => ("✓", GREEN),
                MergeStatus::Blocked => ("⊘", RED),
                MergeStatus::Conflicts => ("⚡", RED),
                MergeStatus::Behind => ("⇣", YELLOW),
                MergeStatus::Unstable => ("⚠", YELLOW),
                MergeStatus::Unknown => ("…", OVERLAY_TEXT),
            };

            // Age + stale
            let age = pr.age_string();
            let age_cell = if pr.is_stale() {
                Cell::from(Line::from(vec![
                    Span::styled(age, Style::default().fg(YELLOW)),
                    Span::styled(" ⏳", Style::default().fg(YELLOW)),
                ]))
            } else {
                Cell::from(Span::styled(age, Style::default().fg(SUBTEXT)))
            };

            let row = Row::new(vec![
                repo_cell,
                Cell::from(pr.title.clone()).style(title_style),
                Cell::from(ci_sym).style(Style::default().fg(ci_color)),
                review_cell,
                Cell::from(merge_sym).style(Style::default().fg(merge_color)),
                age_cell,
            ]);

            if selected {
                row.style(Style::default().bg(SELECTED_BG))
            } else {
                row
            }
        })
        .collect();

    let widths = vec![
        Constraint::Length(24),
        Constraint::Min(20),
        Constraint::Length(4),
        Constraint::Length(12),
        Constraint::Length(7),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().padding(Padding::horizontal(1)));

    frame.render_widget(table, area);
}

// ── Review Requests table ───────────────────────────────────────────

fn draw_reviews_table(frame: &mut Frame, area: Rect, app: &App) {
    if app.review_requests.is_empty() {
        draw_centered_message(frame, area, "No pending review requests", OVERLAY_TEXT);
        return;
    }

    let header = Row::new(vec![
        Cell::from("   REPO"),
        Cell::from("TITLE"),
        Cell::from("AUTHOR"),
        Cell::from("REQUESTED"),
        Cell::from(""),
    ])
    .style(Style::default().fg(OVERLAY_TEXT).bold())
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .review_requests
        .iter()
        .enumerate()
        .map(|(i, rr)| {
            let selected = i == app.selected_index();

            let rc = repo_color(&rr.repo);
            let (accent, repo_style, title_style) = if selected {
                (
                    Span::styled(" ▎ ", Style::default().fg(BLUE)),
                    Style::default().fg(rc).bold(),
                    Style::default().fg(TEXT).bold(),
                )
            } else {
                (
                    Span::styled("   ", Style::default()),
                    Style::default().fg(rc),
                    Style::default().fg(TEXT),
                )
            };

            let repo_cell = Cell::from(Line::from(vec![
                accent,
                Span::styled(rr.repo.clone(), repo_style),
            ]));

            let direct_cell = if rr.is_direct {
                Cell::from(Span::styled("●", Style::default().fg(LAVENDER)))
            } else {
                Cell::from(Span::styled("●", Style::default().fg(OVERLAY_TEXT)))
            };

            let row = Row::new(vec![
                repo_cell,
                Cell::from(rr.title.clone()).style(title_style),
                Cell::from(rr.author.clone()).style(Style::default().fg(SUBTEXT)),
                Cell::from(rr.age_string()).style(Style::default().fg(SUBTEXT)),
                direct_cell,
            ]);

            if selected {
                row.style(Style::default().bg(SELECTED_BG))
            } else {
                row
            }
        })
        .collect();

    let widths = vec![
        Constraint::Length(24),
        Constraint::Min(20),
        Constraint::Length(18),
        Constraint::Length(12),
        Constraint::Length(3),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().padding(Padding::horizontal(1)));

    frame.render_widget(table, area);
}

// ── Loading state ───────────────────────────────────────────────────

fn draw_loading(frame: &mut Frame, area: Rect, tick: u64) {
    let spinner = SPINNER[(tick as usize / 3) % SPINNER.len()];

    let msg = Line::from(vec![
        Span::styled(format!("{}  ", spinner), Style::default().fg(BLUE)),
        Span::styled(
            "Fetching data…",
            Style::default().fg(SUBTEXT),
        ),
    ]);

    let centered = vertical_center(area, 1);
    frame.render_widget(Paragraph::new(msg).alignment(Alignment::Center), centered);
}

// ── Error state ─────────────────────────────────────────────────────

fn draw_error(frame: &mut Frame, area: Rect, message: &str) {
    let lines = vec![
        Line::from(Span::styled(
            format!("  {}", message),
            Style::default().fg(RED),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press r to retry",
            Style::default().fg(OVERLAY_TEXT),
        )),
    ];

    let centered = vertical_center(area, 3);
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        centered,
    );
}

// ── Help overlay ────────────────────────────────────────────────────

fn draw_help_overlay(frame: &mut Frame, area: Rect) {
    let width = 44u16;
    let height = 30u16;
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let overlay = Rect::new(x, y, width.min(area.width), height.min(area.height));

    frame.render_widget(Clear, overlay);

    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  KEYBINDINGS",
            Style::default().fg(LAVENDER).bold(),
        )),
        Line::from(""),
        help_line("  ↑ / ↓     ", "Navigate rows"),
        help_line("  Enter     ", "Open PR in browser"),
        help_line("  Tab       ", "Switch view"),
        help_line("  1 / 2     ", "Jump to view"),
        help_line("  r         ", "Refresh data"),
        help_line("  s         ", "Toggle sort order"),
        help_line("  q / Esc   ", "Quit"),
        Line::from(""),
        Line::from(Span::styled(
            "  REVIEWS",
            Style::default().fg(LAVENDER).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ●", Style::default().fg(LAVENDER)),
            Span::styled(" you directly  ", Style::default().fg(SUBTEXT)),
            Span::styled("●", Style::default().fg(OVERLAY_TEXT)),
            Span::styled(" via team", Style::default().fg(SUBTEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  MERGE STATUS",
            Style::default().fg(LAVENDER).bold(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ✓", Style::default().fg(GREEN)),
            Span::styled(" Ready    ", Style::default().fg(SUBTEXT)),
            Span::styled("⊘", Style::default().fg(RED)),
            Span::styled(" Blocked", Style::default().fg(SUBTEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ⚡", Style::default().fg(RED)),
            Span::styled(" Conflicts ", Style::default().fg(SUBTEXT)),
            Span::styled("⇣", Style::default().fg(YELLOW)),
            Span::styled(" Behind", Style::default().fg(SUBTEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ⚠", Style::default().fg(YELLOW)),
            Span::styled(" Unstable  ", Style::default().fg(SUBTEXT)),
            Span::styled("…", Style::default().fg(OVERLAY_TEXT)),
            Span::styled(" Unknown", Style::default().fg(SUBTEXT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Press any key to close",
            Style::default().fg(OVERLAY_TEXT),
        )),
        Line::from(""),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(SURFACE))
        .title(Span::styled(
            " Help ",
            Style::default().fg(LAVENDER).bold(),
        ))
        .title_alignment(Alignment::Center);

    frame.render_widget(Paragraph::new(help_text).block(block), overlay);
}

fn help_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(key, Style::default().fg(TEXT).bold()),
        Span::styled(desc, Style::default().fg(SUBTEXT)),
    ])
}

// ── Utilities ───────────────────────────────────────────────────────

fn draw_centered_message(frame: &mut Frame, area: Rect, msg: &str, color: Color) {
    let centered = vertical_center(area, 1);
    let p = Paragraph::new(Span::styled(msg, Style::default().fg(color)))
        .alignment(Alignment::Center);
    frame.render_widget(p, centered);
}

fn vertical_center(area: Rect, height: u16) -> Rect {
    let y = area.y + area.height / 2;
    Rect {
        y: y.saturating_sub(height / 2),
        height: height.min(area.height),
        ..area
    }
}

fn inset_horizontal(area: Rect, amount: u16) -> Rect {
    Rect {
        x: area.x + amount,
        width: area.width.saturating_sub(amount * 2),
        ..area
    }
}
