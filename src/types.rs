use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub enum CiStatus {
    Passing,
    Failing,
    Pending,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewStatus {
    Approved,
    ChangesRequested,
    Pending,
    NoReviewers,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MergeStatus {
    Ready,
    Blocked,
    Conflicts,
    Behind,
    Unstable,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub repo: String,
    pub title: String,
    pub url: String,
    pub ci_status: CiStatus,
    pub review_status: ReviewStatus,
    pub merge_status: MergeStatus,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub is_draft: bool,
}

impl PullRequest {
    pub fn is_stale(&self) -> bool {
        let days = (Utc::now() - self.updated_at).num_days();
        days >= 7
    }

    pub fn age_string(&self) -> String {
        format_relative_time(self.created_at)
    }
}

#[derive(Debug, Clone)]
pub struct ReviewRequest {
    pub repo: String,
    pub title: String,
    pub url: String,
    pub author: String,
    pub requested_at: DateTime<Utc>,
    pub is_direct: bool,
}

impl ReviewRequest {
    pub fn age_string(&self) -> String {
        format_relative_time(self.requested_at)
    }
}

fn format_relative_time(dt: DateTime<Utc>) -> String {
    let duration = Utc::now() - dt;
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 30 {
        format!("{}d ago", days)
    } else {
        format!("{}mo ago", days / 30)
    }
}
