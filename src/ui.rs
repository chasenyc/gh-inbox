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

pub fn draw(frame: &mut Frame, app: &mut App) {
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
        tab_spans("My PRs", Some(app.my_prs.len()), app.tab == Tab::MyPrs);
    let (rev_dot, rev_label, rev_count_s) =
        tab_spans("Reviews", Some(app.review_requests.len()), app.tab == Tab::ReviewRequests);
    let (stats_dot, stats_label, stats_count_s) =
        tab_spans("Stats", None, app.tab == Tab::Stats);

    let line = Line::from(vec![
        Span::raw(" "),
        my_dot,
        my_label,
        my_count_s,
        Span::raw("       "),
        rev_dot,
        rev_label,
        rev_count_s,
        Span::raw("       "),
        stats_dot,
        stats_label,
        stats_count_s,
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn tab_spans<'a>(label: &'a str, count: Option<usize>, active: bool) -> (Span<'a>, Span<'a>, Span<'a>) {
    let count_str = match count {
        Some(c) => format!("  {}", c),
        None => String::new(),
    };
    if active {
        (
            Span::styled("● ", Style::default().fg(BLUE)),
            Span::styled(label, Style::default().fg(TEXT).bold()),
            Span::styled(count_str, Style::default().fg(BLUE)),
        )
    } else {
        (
            Span::styled("○ ", Style::default().fg(OVERLAY_TEXT)),
            Span::styled(label, Style::default().fg(OVERLAY_TEXT)),
            Span::styled(count_str, Style::default().fg(OVERLAY_TEXT)),
        )
    }
}

// ── Content dispatch ────────────────────────────────────────────────

fn draw_content(frame: &mut Frame, area: Rect, app: &mut App) {
    match app.tab {
        Tab::MyPrs => draw_my_prs_table(frame, area, app),
        Tab::ReviewRequests => draw_reviews_table(frame, area, app),
        Tab::Stats => draw_stats(frame, area, app),
    }
}

// ── My PRs table ────────────────────────────────────────────────────

fn draw_my_prs_table(frame: &mut Frame, area: Rect, app: &mut App) {
    if app.my_prs.is_empty() {
        if app.snake_game.is_some() {
            draw_snake_game(frame, area, app);
        } else {
            draw_empty_prs(frame, area);
        }
        return;
    }
    // Clear snake game if PRs appeared
    app.snake_game = None;

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

            Row::new(vec![
                repo_cell,
                Cell::from(pr.title.clone()).style(title_style),
                Cell::from(ci_sym).style(Style::default().fg(ci_color)),
                review_cell,
                Cell::from(merge_sym).style(Style::default().fg(merge_color)),
                age_cell,
            ])
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
        .block(Block::default().padding(Padding::horizontal(1)))
        .row_highlight_style(Style::default().bg(SELECTED_BG))
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Never);

    frame.render_stateful_widget(table, area, &mut app.my_prs_table_state);
}

// ── Review Requests table ───────────────────────────────────────────

fn draw_reviews_table(frame: &mut Frame, area: Rect, app: &mut App) {
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

            Row::new(vec![
                repo_cell,
                Cell::from(rr.title.clone()).style(title_style),
                Cell::from(rr.author.clone()).style(Style::default().fg(SUBTEXT)),
                Cell::from(rr.age_string()).style(Style::default().fg(SUBTEXT)),
                direct_cell,
            ])
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
        .block(Block::default().padding(Padding::horizontal(1)))
        .row_highlight_style(Style::default().bg(SELECTED_BG))
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Never);

    frame.render_stateful_widget(table, area, &mut app.reviews_table_state);
}

// ── Empty PR state ──────────────────────────────────────────────────

fn draw_empty_prs(frame: &mut Frame, area: Rect) {
    let centered = vertical_center(area, 3);
    let lines = vec![
        Line::from(Span::styled(
            "No open PRs — nice work!",
            Style::default().fg(SUBTEXT),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "⎵",
            Style::default().fg(SUBTEXT).bold(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        centered,
    );
}

// ── Snake game ──────────────────────────────────────────────────────

const SNAKE_HEAD: &str = "██";
const SNAKE_BODY: &str = "██";
const SNAKE_FOOD: &str = "⎇ ";

fn draw_snake_game(frame: &mut Frame, area: Rect, app: &mut App) {
    let game = match app.snake_game.as_mut() {
        Some(g) => g,
        None => return,
    };

    let game_w = game.width;
    let game_h = game.height;

    let needed_w = game_w * 2 + 2;
    let needed_h = game_h + 4;
    if area.width < needed_w || area.height < needed_h {
        draw_centered_message(frame, area, "Window too small for snake!", OVERLAY_TEXT);
        return;
    }

    // Border block
    let game_pixel_w = game_w * 2 + 2; // +2 for borders
    let game_pixel_h = game_h + 2; // +2 for borders
    let x = area.x + (area.width.saturating_sub(game_pixel_w)) / 2;
    let y = area.y + (area.height.saturating_sub(game_pixel_h + 2)) / 2; // +2 for score line

    let game_area = Rect::new(x, y, game_pixel_w, game_pixel_h);

    let score_text = format!(" Score: {} ", game.score);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(Span::styled(
            " eat the branch ",
            Style::default().fg(LAVENDER).bold(),
        ))
        .title(
            Line::from(Span::styled(score_text, Style::default().fg(GREEN)))
                .right_aligned(),
        );

    let inner = block.inner(game_area);
    frame.render_widget(block, game_area);

    // Draw food
    let food = &game.food;
    let fx = inner.x + food.x as u16 * 2;
    let fy = inner.y + food.y as u16;
    if fx + 2 <= inner.x + inner.width && fy < inner.y + inner.height {
        frame.render_widget(
            Paragraph::new(Span::styled(SNAKE_FOOD, Style::default().fg(RED))),
            Rect::new(fx, fy, 2, 1),
        );
    }

    // Draw snake
    for (i, seg) in game.snake.iter().enumerate() {
        let sx = inner.x + seg.x as u16 * 2;
        let sy = inner.y + seg.y as u16;
        if sx + 2 > inner.x + inner.width || sy >= inner.y + inner.height {
            continue;
        }
        let (ch, color) = if i == 0 {
            (SNAKE_HEAD, BLUE)
        } else {
            (SNAKE_BODY, LAVENDER)
        };
        frame.render_widget(
            Paragraph::new(Span::styled(ch, Style::default().fg(color))),
            Rect::new(sx, sy, 2, 1),
        );
    }

    // Game over overlay
    if game.game_over {
        let msg_y = y + game_pixel_h + 1;
        if msg_y < area.y + area.height {
            let lines = vec![Line::from(vec![
                Span::styled("Game Over! ", Style::default().fg(RED).bold()),
                Span::styled("space", Style::default().fg(SUBTEXT).bold()),
                Span::styled(" restart  ", Style::default().fg(OVERLAY_TEXT)),
                Span::styled("q", Style::default().fg(SUBTEXT).bold()),
                Span::styled(" quit", Style::default().fg(OVERLAY_TEXT)),
            ])];
            frame.render_widget(
                Paragraph::new(lines).alignment(Alignment::Center),
                Rect::new(area.x, msg_y, area.width, 1),
            );
        }
    } else {
        let hint_y = y + game_pixel_h + 1;
        if hint_y < area.y + area.height {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    "arrow keys to move · q to quit",
                    Style::default().fg(OVERLAY_TEXT),
                ))
                .alignment(Alignment::Center),
                Rect::new(area.x, hint_y, area.width, 1),
            );
        }
    }
}

// ── Stats view ──────────────────────────────────────────────────────

const SURFACE_BRIGHT: Color = Color::Rgb(49, 50, 68);

// Block elements for sub-cell resolution: each gives 1/8th fill
const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn draw_stats(frame: &mut Frame, area: Rect, app: &mut App) {
    let (merged, reviewed) = match (&app.merged_stats, &app.reviewed_stats) {
        (Some(m), Some(r)) => (m, r),
        _ => {
            draw_loading(frame, area, app.tick);
            return;
        }
    };

    let chunks = Layout::vertical([
        Constraint::Length(5), // metric cards
        Constraint::Min(8),   // charts
    ])
    .split(area);

    draw_metric_cards(frame, chunks[0], merged, reviewed);

    let chart_cols = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(chunks[1]);

    draw_gradient_chart(frame, chart_cols[0], "PRs Merged", "ship it 🚀", merged, &[
        Color::Rgb(22, 78, 56),    // dark green
        Color::Rgb(34, 120, 80),
        Color::Rgb(74, 172, 114),
        Color::Rgb(166, 218, 149), // bright green
    ]);
    draw_gradient_chart(frame, chart_cols[1], "PRs Reviewed", "unblocking others", reviewed, &[
        Color::Rgb(60, 46, 100),   // dark purple
        Color::Rgb(100, 72, 160),
        Color::Rgb(150, 120, 210),
        Color::Rgb(180, 190, 254), // bright lavender
    ]);
}

fn draw_metric_cards(
    frame: &mut Frame,
    area: Rect,
    merged: &crate::types::WeeklyStats,
    reviewed: &crate::types::WeeklyStats,
) {
    let card_area = inset_horizontal(area, 1);
    let cards = Layout::horizontal([
        Constraint::Ratio(1, 6),
        Constraint::Ratio(1, 6),
        Constraint::Ratio(1, 6),
        Constraint::Ratio(1, 6),
        Constraint::Ratio(1, 6),
        Constraint::Ratio(1, 6),
    ])
    .horizontal_margin(1)
    .split(card_area);

    let merged_trend = merged.trend();
    let reviewed_trend = reviewed.trend();

    draw_metric_card(frame, cards[0], "MERGED", &merged.total().to_string(), "total", GREEN);
    draw_metric_card(frame, cards[1], "AVG/WEEK", &format!("{:.1}", merged.avg_per_week()), "merged", GREEN);
    draw_metric_card(frame, cards[2], "THIS WEEK", &merged.current_week().to_string(), trend_label(merged_trend), trend_color(merged_trend));
    draw_metric_card(frame, cards[3], "REVIEWED", &reviewed.total().to_string(), "total", LAVENDER);
    draw_metric_card(frame, cards[4], "AVG/WEEK", &format!("{:.1}", reviewed.avg_per_week()), "reviewed", LAVENDER);
    draw_metric_card(frame, cards[5], "THIS WEEK", &reviewed.current_week().to_string(), trend_label(reviewed_trend), trend_color(reviewed_trend));
}

fn trend_label(trend: &str) -> &str {
    match trend {
        "up" => "trending up",
        "down" => "trending down",
        "stable" => "stable",
        _ => "",
    }
}

fn trend_color(trend: &str) -> Color {
    match trend {
        "up" => GREEN,
        "down" => RED,
        "stable" => SUBTEXT,
        _ => OVERLAY_TEXT,
    }
}

fn draw_metric_card(frame: &mut Frame, area: Rect, label: &str, value: &str, subtitle: &str, accent: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE_BRIGHT))
        .style(Style::default());

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 || inner.width < 4 {
        return;
    }

    let lines = vec![
        Line::from(Span::styled(label, Style::default().fg(OVERLAY_TEXT))).centered(),
        Line::from(Span::styled(value, Style::default().fg(accent).bold())).centered(),
        Line::from(Span::styled(subtitle, Style::default().fg(OVERLAY_TEXT))).centered(),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_gradient_chart(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    subtitle: &str,
    stats: &crate::types::WeeklyStats,
    gradient: &[Color],
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(SURFACE_BRIGHT))
        .style(Style::default())
        .title(Line::from(vec![
            Span::styled(format!(" {} ", title), Style::default().fg(TEXT).bold()),
            Span::styled(format!(" {} ", subtitle), Style::default().fg(OVERLAY_TEXT)),
        ]))
        .padding(Padding::new(2, 2, 1, 0));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    // Reserve space: 1 row for labels, 1 row for axis line
    let chart_height = inner.height.saturating_sub(2) as usize;
    let max_val = stats.max().max(1);

    let num_weeks = stats.weeks.len();
    // Each slot = bar chars + 1 gap. Y-axis takes 4 chars (3 digits + 1 space).
    let y_axis_width = 4usize;
    let slot_width = ((inner.width as usize).saturating_sub(y_axis_width)) / num_weeks.max(1);
    let slot_width = slot_width.max(2).min(7);
    let bar_width = slot_width.saturating_sub(1).max(1); // bar chars within slot (rest is gap)

    // Build each row of the chart from top to bottom
    for row in 0..chart_height {
        let y = inner.y + row as u16;
        let row_from_bottom = chart_height - 1 - row;
        // This row represents values in range [row_from_bottom * max_val / chart_height, ...]
        let threshold = row_from_bottom as f64 * max_val as f64 / chart_height as f64;

        let mut spans: Vec<Span> = Vec::new();

        // Y-axis label (4 chars wide: 3 digits + 1 space)
        let mid_row = chart_height / 2;
        let mid_val = max_val / 2;
        if row == 0 {
            spans.push(Span::styled(
                format!("{:>3} ", max_val),
                Style::default().fg(OVERLAY_TEXT),
            ));
        } else if row == mid_row && mid_val > 0 {
            spans.push(Span::styled(
                format!("{:>3} ", mid_val),
                Style::default().fg(OVERLAY_TEXT),
            ));
        } else if row == chart_height - 1 {
            spans.push(Span::styled("  0 ", Style::default().fg(OVERLAY_TEXT)));
        } else {
            spans.push(Span::raw("    "));
        }

        for (_i, week) in stats.weeks.iter().enumerate() {
            let val = week.count as f64;
            // How much of this cell is filled?
            let cell_bottom = threshold;
            let cell_top = cell_bottom + max_val as f64 / chart_height as f64;

            let fill_fraction = if val >= cell_top {
                1.0
            } else if val > cell_bottom {
                (val - cell_bottom) / (cell_top - cell_bottom)
            } else {
                0.0
            };

            let block_index = (fill_fraction * 8.0).round() as usize;
            let block_index = block_index.min(8);
            let ch = BLOCKS[block_index];

            // Gradient color based on height position
            let gradient_pos = if chart_height > 1 {
                row_from_bottom as f64 / (chart_height - 1) as f64
            } else {
                1.0
            };
            let color = gradient_color(gradient, gradient_pos);

            let bar_str: String = std::iter::repeat(ch).take(bar_width).collect();

            if block_index > 0 {
                spans.push(Span::styled(bar_str, Style::default().fg(color)));
            } else {
                spans.push(Span::raw(" ".repeat(bar_width)));
            }

            // Gap between bars (1 char to fill the slot)
            let gap = slot_width - bar_width;
            if gap > 0 {
                spans.push(Span::raw(" ".repeat(gap)));
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect::new(inner.x, y, inner.width, 1),
        );
    }

    // Axis line
    let axis_y = inner.y + chart_height as u16;
    let axis_line: String = " ".repeat(y_axis_width)
        + &"─".repeat(inner.width.saturating_sub(y_axis_width as u16) as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(axis_line, Style::default().fg(SURFACE_BRIGHT))),
        Rect::new(inner.x, axis_y, inner.width, 1),
    );

    // Week labels
    let label_y = inner.y + chart_height as u16 + 1;
    if label_y < inner.y + inner.height {
        let mut label_spans: Vec<Span> = Vec::new();
        label_spans.push(Span::raw(" ".repeat(y_axis_width))); // y-axis offset

        for (i, _) in stats.weeks.iter().enumerate() {
            let label = stats.label(i);
            // Only show every Nth label to avoid crowding
            let show_label = if num_weeks <= 6 {
                true
            } else if num_weeks <= 12 {
                i % 2 == 0 || i == num_weeks - 1
            } else {
                i % 3 == 0 || i == num_weeks - 1
            };

            if show_label {
                let padded = format!("{:<width$}", label, width = slot_width);
                label_spans.push(Span::styled(
                    padded.chars().take(slot_width).collect::<String>(),
                    Style::default().fg(OVERLAY_TEXT),
                ));
            } else {
                label_spans.push(Span::raw(" ".repeat(slot_width)));
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(label_spans)),
            Rect::new(inner.x, label_y, inner.width, 1),
        );
    }
}

fn gradient_color(gradient: &[Color], t: f64) -> Color {
    if gradient.is_empty() {
        return TEXT;
    }
    if gradient.len() == 1 {
        return gradient[0];
    }

    let t = t.clamp(0.0, 1.0);
    let segment = t * (gradient.len() - 1) as f64;
    let i = (segment as usize).min(gradient.len() - 2);
    let frac = segment - i as f64;

    let (r1, g1, b1) = extract_rgb(gradient[i]);
    let (r2, g2, b2) = extract_rgb(gradient[i + 1]);

    Color::Rgb(
        lerp_u8(r1, r2, frac),
        lerp_u8(g1, g2, frac),
        lerp_u8(b1, b2, frac),
    )
}

fn extract_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (128, 128, 128),
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
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
        help_line("  1 / 2 / 3 ", "Jump to view"),
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
