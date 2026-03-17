use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use std::process::Command;

use crate::types::{CiStatus, MergeStatus, PullRequest, ReviewRequest, ReviewStatus, WeeklyStats};

const GITHUB_API: &str = "https://api.github.com";

pub struct GitHubClient {
    client: Client,
    token: String,
    username: String,
}

// --- API response types ---

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    title: String,
    html_url: String,
    user: Option<GitHubUser>,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    draft: Option<bool>,
    repository_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Deserialize)]
struct CheckRunsResponse {
    check_runs: Vec<CheckRun>,
}

#[derive(Deserialize)]
struct CheckRun {
    status: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct Review {
    state: String,
    user: Option<GitHubUser>,
}

#[derive(Deserialize)]
struct PrDetail {
    #[serde(default)]
    requested_reviewers: Vec<GitHubUser>,
    #[serde(default)]
    requested_teams: Vec<TeamRef>,
    mergeable_state: Option<String>,
}

#[derive(Deserialize)]
struct TeamRef {
    #[allow(dead_code)]
    slug: String,
}

#[derive(Deserialize)]
struct CommitRef {
    sha: String,
}

// --- Implementation ---

impl GitHubClient {
    pub fn new() -> Result<Self> {
        let token = get_gh_token()?;
        let username = get_gh_username(&token)?;
        let client = Client::new();
        Ok(Self {
            client,
            token,
            username,
        })
    }

    fn auth_get(&self, url: &str) -> reqwest::RequestBuilder {
        self.client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, "gh-inbox")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    pub async fn fetch_my_prs(&self) -> Result<Vec<PullRequest>> {
        let query = format!(
            "author:{} type:pr state:open archived:false sort:updated-desc",
            self.username
        );
        let url = format!("{}/search/issues?q={}&per_page=100", GITHUB_API, urlencoded(&query));

        let resp: SearchResponse = self
            .auth_get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut prs: Vec<PullRequest> = resp
            .items
            .into_iter()
            .map(|item| {
                let repo = extract_repo(&item.repository_url);
                PullRequest {
                    repo,
                    title: item.title,
                    url: item.html_url,
                    ci_status: CiStatus::None,
                    review_status: ReviewStatus::NoReviewers,
                    merge_status: MergeStatus::Unknown,
                    updated_at: item.updated_at,
                    created_at: item.created_at,
                    is_draft: item.draft.unwrap_or(false),
                }
            })
            .collect();

        prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(prs)
    }

    pub async fn fetch_review_requests(&self) -> Result<Vec<ReviewRequest>> {
        let query = format!(
            "review-requested:{} type:pr state:open archived:false sort:updated-desc",
            self.username
        );
        let url = format!("{}/search/issues?q={}&per_page=100", GITHUB_API, urlencoded(&query));

        let resp: SearchResponse = self
            .auth_get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let requests: Vec<ReviewRequest> = resp
            .items
            .into_iter()
            .map(|item| {
                let repo = extract_repo(&item.repository_url);
                let author = item
                    .user
                    .map(|u| u.login)
                    .unwrap_or_else(|| "unknown".to_string());
                ReviewRequest {
                    repo,
                    title: item.title,
                    url: item.html_url,
                    author,
                    requested_at: item.created_at,
                    is_direct: false,
                    is_draft: item.draft.unwrap_or(false),
                    ci_status: CiStatus::None,
                    merge_status: MergeStatus::Unknown,
                }
            })
            .collect();

        Ok(requests)
    }

    pub async fn fetch_ci_status(&self, repo: &str, pr_url: &str) -> CiStatus {
        let number = match pr_url.rsplit('/').next().and_then(|n| n.parse::<u64>().ok()) {
            Some(n) => n,
            None => return CiStatus::None,
        };

        // Get the head SHA from the PR commits endpoint
        let commits_url = format!("{}/repos/{}/pulls/{}/commits", GITHUB_API, repo, number);
        let commits: Vec<CommitRef> = match self
            .auth_get(&commits_url)
            .query(&[("per_page", "1")])
            .send()
            .await
            .and_then(|r| r.error_for_status().map_err(Into::into))
        {
            Ok(resp) => match resp.json().await {
                Ok(c) => c,
                Err(_) => return CiStatus::None,
            },
            Err(_) => return CiStatus::None,
        };

        let sha = match commits.last() {
            Some(c) => &c.sha,
            None => return CiStatus::None,
        };

        let url = format!(
            "{}/repos/{}/commits/{}/check-runs?per_page=100",
            GITHUB_API, repo, sha
        );

        let resp: CheckRunsResponse = match self
            .auth_get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status().map_err(Into::into))
        {
            Ok(resp) => match resp.json().await {
                Ok(r) => r,
                Err(_) => return CiStatus::None,
            },
            Err(_) => return CiStatus::None,
        };

        if resp.check_runs.is_empty() {
            return CiStatus::None;
        }

        let any_failing = resp.check_runs.iter().any(|c| {
            c.conclusion
                .as_deref()
                .is_some_and(|s| s == "failure" || s == "timed_out" || s == "cancelled")
        });
        if any_failing {
            return CiStatus::Failing;
        }

        let any_pending = resp
            .check_runs
            .iter()
            .any(|c| c.status == "queued" || c.status == "in_progress");
        if any_pending {
            return CiStatus::Pending;
        }

        CiStatus::Passing
    }

    pub async fn fetch_review_and_merge_status(
        &self,
        repo: &str,
        pr_url: &str,
    ) -> (ReviewStatus, MergeStatus) {
        let number = match pr_url.rsplit('/').next().and_then(|n| n.parse::<u64>().ok()) {
            Some(n) => n,
            None => return (ReviewStatus::NoReviewers, MergeStatus::Unknown),
        };

        // Fetch PR details (reviewers + mergeable_state)
        let detail_url = format!("{}/repos/{}/pulls/{}", GITHUB_API, repo, number);
        let detail: PrDetail = match self
            .auth_get(&detail_url)
            .send()
            .await
            .and_then(|r| r.error_for_status().map_err(Into::into))
        {
            Ok(resp) => match resp.json().await {
                Ok(d) => d,
                Err(_) => return (ReviewStatus::NoReviewers, MergeStatus::Unknown),
            },
            Err(_) => return (ReviewStatus::NoReviewers, MergeStatus::Unknown),
        };

        // Extract merge status from the same response
        let merge_status = match detail.mergeable_state.as_deref() {
            Some("clean") => MergeStatus::Ready,
            Some("blocked") => MergeStatus::Blocked,
            Some("dirty") => MergeStatus::Conflicts,
            Some("behind") => MergeStatus::Behind,
            Some("unstable") => MergeStatus::Unstable,
            _ => MergeStatus::Unknown,
        };

        // Fetch submitted reviews
        let reviews_url = format!(
            "{}/repos/{}/pulls/{}/reviews?per_page=100",
            GITHUB_API, repo, number
        );
        let reviews: Vec<Review> = match self
            .auth_get(&reviews_url)
            .send()
            .await
            .and_then(|r| r.error_for_status().map_err(Into::into))
        {
            Ok(resp) => match resp.json().await {
                Ok(r) => r,
                Err(_) => return (ReviewStatus::NoReviewers, merge_status),
            },
            Err(_) => return (ReviewStatus::NoReviewers, merge_status),
        };

        let has_reviewers = !detail.requested_reviewers.is_empty()
            || !detail.requested_teams.is_empty()
            || !reviews.is_empty();

        if !has_reviewers {
            return (ReviewStatus::NoReviewers, merge_status);
        }

        // Keep only the latest review per author (reviews are returned chronologically)
        let mut latest_by_author: std::collections::HashMap<String, &str> =
            std::collections::HashMap::new();
        for review in &reviews {
            if review.state == "COMMENTED" {
                continue;
            }
            let author = review
                .user
                .as_ref()
                .map(|u| u.login.clone())
                .unwrap_or_default();
            latest_by_author.insert(author, &review.state);
        }

        let has_approved = latest_by_author.values().any(|&s| s == "APPROVED");
        let has_changes = latest_by_author.values().any(|&s| s == "CHANGES_REQUESTED");

        let review_status = if has_changes {
            ReviewStatus::ChangesRequested
        } else if has_approved {
            ReviewStatus::Approved
        } else {
            ReviewStatus::Pending
        };

        (review_status, merge_status)
    }

    pub async fn fetch_is_direct_request(&self, repo: &str, pr_url: &str) -> (bool, MergeStatus) {
        let number = match pr_url.rsplit('/').next().and_then(|n| n.parse::<u64>().ok()) {
            Some(n) => n,
            None => return (false, MergeStatus::Unknown),
        };

        let detail_url = format!("{}/repos/{}/pulls/{}", GITHUB_API, repo, number);
        let detail: PrDetail = match self
            .auth_get(&detail_url)
            .send()
            .await
            .and_then(|r| r.error_for_status().map_err(Into::into))
        {
            Ok(resp) => match resp.json().await {
                Ok(d) => d,
                Err(_) => return (false, MergeStatus::Unknown),
            },
            Err(_) => return (false, MergeStatus::Unknown),
        };

        let is_direct = detail
            .requested_reviewers
            .iter()
            .any(|u| u.login.eq_ignore_ascii_case(&self.username));

        let merge_status = match detail.mergeable_state.as_deref() {
            Some("clean") => MergeStatus::Ready,
            Some("blocked") => MergeStatus::Blocked,
            Some("dirty") => MergeStatus::Conflicts,
            Some("behind") => MergeStatus::Behind,
            Some("unstable") => MergeStatus::Unstable,
            _ => MergeStatus::Unknown,
        };

        (is_direct, merge_status)
    }

    pub async fn fetch_merged_prs_stats(&self, num_weeks: usize) -> WeeklyStats {
        let since = chrono::Utc::now() - chrono::Duration::weeks(num_weeks as i64);
        let since_str = since.format("%Y-%m-%d").to_string();
        let query = format!(
            "author:{} type:pr is:merged archived:false merged:>{}",
            self.username, since_str
        );

        let dates = self.fetch_search_dates(&query, "closed_at").await;
        WeeklyStats::from_dates(&dates, num_weeks)
    }

    pub async fn fetch_reviewed_prs_stats(&self, num_weeks: usize) -> WeeklyStats {
        let since = chrono::Utc::now() - chrono::Duration::weeks(num_weeks as i64);
        let since_str = since.format("%Y-%m-%d").to_string();
        let query = format!(
            "reviewed-by:{} type:pr is:merged archived:false -author:{} merged:>{}",
            self.username, self.username, since_str
        );

        let dates = self.fetch_search_dates(&query, "closed_at").await;
        WeeklyStats::from_dates(&dates, num_weeks)
    }

    async fn fetch_search_dates(&self, query: &str, date_field: &str) -> Vec<DateTime<Utc>> {
        let mut all_dates = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/search/issues?q={}&per_page=100&page={}",
                GITHUB_API,
                urlencoded(query),
                page
            );

            let resp: serde_json::Value = match self
                .auth_get(&url)
                .send()
                .await
                .and_then(|r| r.error_for_status().map_err(Into::into))
            {
                Ok(resp) => match resp.json().await {
                    Ok(v) => v,
                    Err(_) => break,
                },
                Err(_) => break,
            };

            let items = match resp.get("items").and_then(|v| v.as_array()) {
                Some(items) => items,
                None => break,
            };

            if items.is_empty() {
                break;
            }

            for item in items {
                if let Some(date_str) = item.get(date_field).and_then(|v| v.as_str()) {
                    if let Ok(dt) = date_str.parse::<DateTime<Utc>>() {
                        all_dates.push(dt);
                    }
                }
            }

            let total = resp.get("total_count").and_then(|v| v.as_u64()).unwrap_or(0);
            if (page as u64) * 100 >= total || page >= 10 {
                break;
            }
            page += 1;
        }

        all_dates
    }
}

#[derive(Deserialize)]
struct ReleaseResponse {
    tag_name: String,
}

/// Check if a newer version is available on GitHub Releases.
/// Returns `Some("x.y.z")` if newer, `None` if current or on error.
pub async fn check_for_update() -> Option<String> {
    let client = Client::new();
    let resp: ReleaseResponse = client
        .get(format!("{}/repos/chasenyc/gh-inbox/releases/latest", GITHUB_API))
        .header(USER_AGENT, "gh-inbox")
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    let latest = resp.tag_name.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");

    if version_is_newer(latest, current) {
        Some(latest.to_string())
    } else {
        None
    }
}

/// Returns true if `a` is a strictly newer semver than `b`.
fn version_is_newer(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u64, u64, u64) {
        let mut parts = s.splitn(3, '.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };
    parse(a) > parse(b)
}

fn get_gh_token() -> Result<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("Failed to run `gh auth token`. Is the GitHub CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Not authenticated with GitHub CLI. Run `gh auth login` first.\n{}",
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_gh_username(token: &str) -> Result<String> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .env("GH_TOKEN", token)
        .output()
        .context("Failed to get GitHub username")?;

    if !output.status.success() {
        bail!("Failed to get GitHub username from `gh api user`");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn extract_repo(repository_url: &Option<String>) -> String {
    repository_url
        .as_deref()
        .and_then(|url| {
            // URL is like https://api.github.com/repos/org/repo
            let parts: Vec<&str> = url.rsplitn(3, '/').collect();
            if parts.len() >= 2 {
                Some(format!("{}/{}", parts[1], parts[0]))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace(':', "%3A")
}
