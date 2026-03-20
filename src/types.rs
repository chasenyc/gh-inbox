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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Priority::Critical => write!(f, "!!"),
            Priority::High => write!(f, "! "),
            Priority::Medium => write!(f, "· "),
            Priority::Low => write!(f, "  "),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotificationReason {
    Mention,
    ReviewRequested,
    CiActivity,
    Assign,
    Comment,
    StateChange,
    Other,
}

impl NotificationReason {
    pub fn from_api_string(s: &str) -> Self {
        match s {
            "mention" => Self::Mention,
            "review_requested" => Self::ReviewRequested,
            "ci_activity" => Self::CiActivity,
            "assign" => Self::Assign,
            "comment" => Self::Comment,
            "state_change" => Self::StateChange,
            _ => Self::Other,
        }
    }
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
    pub priority: Priority,
    pub priority_score: i32,
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
    pub is_draft: bool,
    pub ci_status: CiStatus,
    pub merge_status: MergeStatus,
    pub priority: Priority,
    pub priority_score: i32,
}

impl ReviewRequest {
    pub fn age_string(&self) -> String {
        format_relative_time(self.requested_at)
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: String,
    pub reason: NotificationReason,
    pub subject_title: String,
    pub subject_url: String,
    pub repo: String,
    pub updated_at: DateTime<Utc>,
    pub unread: bool,
    pub pending_read: bool,
    pub priority: Priority,
    pub priority_score: i32,
}

impl Notification {
    pub fn age_string(&self) -> String {
        format_relative_time(self.updated_at)
    }
}

#[derive(Debug, Clone)]
pub struct WeeklyStats {
    pub weeks: Vec<WeekBucket>,
}

#[derive(Debug, Clone)]
pub struct WeekBucket {
    pub week_start: DateTime<Utc>,
    pub count: u64,
}

impl WeeklyStats {
    /// Build weekly buckets from a list of dates, covering the last `num_weeks` weeks.
    pub fn from_dates(dates: &[DateTime<Utc>], num_weeks: usize) -> Self {
        use chrono::{Datelike, Duration, TimeZone};

        let now = Utc::now();
        // Start of current week (Monday)
        let today = now.date_naive();
        let days_since_monday = today.weekday().num_days_from_monday();
        let current_monday = today - Duration::days(days_since_monday as i64);
        let earliest_monday = current_monday - Duration::weeks(num_weeks as i64 - 1);

        let mut weeks: Vec<WeekBucket> = (0..num_weeks)
            .map(|i| {
                let week_start = Utc
                    .from_utc_datetime(&(earliest_monday + Duration::weeks(i as i64)).and_hms_opt(0, 0, 0).unwrap());
                WeekBucket {
                    week_start,
                    count: 0,
                }
            })
            .collect();

        for date in dates {
            let d = date.date_naive();
            let days_from_earliest = (d - earliest_monday).num_days();
            if days_from_earliest < 0 {
                continue;
            }
            let week_index = (days_from_earliest / 7) as usize;
            if week_index < weeks.len() {
                weeks[week_index].count += 1;
            }
        }

        WeeklyStats { weeks }
    }

    pub fn label(&self, index: usize) -> String {
        if let Some(bucket) = self.weeks.get(index) {
            bucket.week_start.format("%m/%d").to_string()
        } else {
            String::new()
        }
    }

    pub fn total(&self) -> u64 {
        self.weeks.iter().map(|w| w.count).sum()
    }

    pub fn max(&self) -> u64 {
        self.weeks.iter().map(|w| w.count).max().unwrap_or(0)
    }

    pub fn avg_per_week(&self) -> f64 {
        if self.weeks.is_empty() {
            0.0
        } else {
            self.total() as f64 / self.weeks.len() as f64
        }
    }

    pub fn current_week(&self) -> u64 {
        self.weeks.last().map(|w| w.count).unwrap_or(0)
    }

    pub fn trend(&self) -> &'static str {
        // Compare last 4 weeks avg vs prior 4 weeks avg
        let len = self.weeks.len();
        if len < 8 {
            return "—";
        }
        let recent: f64 = self.weeks[len - 4..].iter().map(|w| w.count as f64).sum::<f64>() / 4.0;
        let prior: f64 = self.weeks[len - 8..len - 4].iter().map(|w| w.count as f64).sum::<f64>() / 4.0;
        if prior == 0.0 && recent > 0.0 {
            "up"
        } else if recent > prior * 1.1 {
            "up"
        } else if recent < prior * 0.9 {
            "down"
        } else {
            "stable"
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_reason_from_api_string() {
        assert_eq!(NotificationReason::from_api_string("mention"), NotificationReason::Mention);
        assert_eq!(NotificationReason::from_api_string("review_requested"), NotificationReason::ReviewRequested);
        assert_eq!(NotificationReason::from_api_string("ci_activity"), NotificationReason::CiActivity);
        assert_eq!(NotificationReason::from_api_string("assign"), NotificationReason::Assign);
        assert_eq!(NotificationReason::from_api_string("comment"), NotificationReason::Comment);
        assert_eq!(NotificationReason::from_api_string("state_change"), NotificationReason::StateChange);
        assert_eq!(NotificationReason::from_api_string("unknown_thing"), NotificationReason::Other);
        assert_eq!(NotificationReason::from_api_string(""), NotificationReason::Other);
    }

    #[test]
    fn priority_display() {
        assert_eq!(format!("{}", Priority::Critical), "!!");
        assert_eq!(format!("{}", Priority::High), "! ");
        assert_eq!(format!("{}", Priority::Medium), "· ");
        assert_eq!(format!("{}", Priority::Low), "  ");
    }
}
