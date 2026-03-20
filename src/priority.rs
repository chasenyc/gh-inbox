use crate::types::{CiStatus, MergeStatus, NotificationReason, Priority, ReviewStatus};

// Priority signal weights and thresholds (hardcoded for v1)
//
// ┌─────────────────────────────────────────┬────────┐
// │ Signal                                  │ Weight │
// ├─────────────────────────────────────────┼────────┤
// │ CI failing on your PR                   │ +40    │
// │ Changes requested on your PR            │ +30    │
// │ Direct @mention (notification)          │ +25    │
// │ Direct review request (you, not team)   │ +20    │
// │ PR has merge conflicts                  │ +15    │
// │ Team review request                     │ +10    │
// │ New comment on your PR                  │ +10    │
// │ PR is stale (7+ days)                   │ +5     │
// │ PR is draft                             │ -10    │
// ├─────────────────────────────────────────┼────────┤
// │ Critical threshold                      │ >= 40  │
// │ High threshold                          │ >= 25  │
// │ Medium threshold                        │ >= 10  │
// │ Low threshold                           │ < 10   │
// └─────────────────────────────────────────┴────────┘

const W_CI_FAILING: i32 = 40;
const W_CHANGES_REQUESTED: i32 = 30;
const W_DIRECT_MENTION: i32 = 25;
const W_DIRECT_REVIEW_REQUEST: i32 = 20;
const W_MERGE_CONFLICTS: i32 = 15;
const W_TEAM_REVIEW_REQUEST: i32 = 10;
const W_NEW_COMMENT: i32 = 10;
const W_STALE: i32 = 5;
const W_DRAFT: i32 = -10;

const THRESHOLD_CRITICAL: i32 = 40;
const THRESHOLD_HIGH: i32 = 25;
const THRESHOLD_MEDIUM: i32 = 10;

pub struct PriorityContext {
    pub ci_status: CiStatus,
    pub review_status: ReviewStatus,
    pub merge_status: MergeStatus,
    pub is_draft: bool,
    pub is_stale: bool,
    pub is_direct_review_request: bool,
    pub notification_reason: Option<NotificationReason>,
}

pub fn compute_priority(ctx: &PriorityContext) -> (Priority, i32) {
    let mut score: i32 = 0;

    if ctx.ci_status == CiStatus::Failing {
        score += W_CI_FAILING;
    }
    if ctx.review_status == ReviewStatus::ChangesRequested {
        score += W_CHANGES_REQUESTED;
    }
    match &ctx.notification_reason {
        Some(NotificationReason::Mention) => score += W_DIRECT_MENTION,
        Some(NotificationReason::Comment) => score += W_NEW_COMMENT,
        Some(NotificationReason::ReviewRequested) => {
            if ctx.is_direct_review_request {
                score += W_DIRECT_REVIEW_REQUEST;
            } else {
                score += W_TEAM_REVIEW_REQUEST;
            }
        }
        Some(_) => {}
        None => {
            if ctx.is_direct_review_request {
                score += W_DIRECT_REVIEW_REQUEST;
            }
        }
    }
    if ctx.merge_status == MergeStatus::Conflicts {
        score += W_MERGE_CONFLICTS;
    }
    if ctx.is_stale {
        score += W_STALE;
    }
    if ctx.is_draft {
        score += W_DRAFT;
    }

    let priority = if score >= THRESHOLD_CRITICAL {
        Priority::Critical
    } else if score >= THRESHOLD_HIGH {
        Priority::High
    } else if score >= THRESHOLD_MEDIUM {
        Priority::Medium
    } else {
        Priority::Low
    };

    (priority, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx() -> PriorityContext {
        PriorityContext {
            ci_status: CiStatus::None,
            review_status: ReviewStatus::NoReviewers,
            merge_status: MergeStatus::Unknown,
            is_draft: false,
            is_stale: false,
            is_direct_review_request: false,
            notification_reason: None,
        }
    }

    #[test]
    fn empty_context_is_low() {
        let (p, s) = compute_priority(&default_ctx());
        assert_eq!(p, Priority::Low);
        assert_eq!(s, 0);
    }

    #[test]
    fn ci_failing_is_critical() {
        let ctx = PriorityContext { ci_status: CiStatus::Failing, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Critical);
        assert_eq!(s, 40);
    }

    #[test]
    fn changes_requested_is_high() {
        let ctx = PriorityContext { review_status: ReviewStatus::ChangesRequested, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::High);
        assert_eq!(s, 30);
    }

    #[test]
    fn direct_mention_is_high() {
        let ctx = PriorityContext {
            notification_reason: Some(NotificationReason::Mention),
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::High);
        assert_eq!(s, 25);
    }

    #[test]
    fn direct_review_request_is_medium() {
        let ctx = PriorityContext { is_direct_review_request: true, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Medium);
        assert_eq!(s, 20);
    }

    #[test]
    fn merge_conflicts_is_medium() {
        let ctx = PriorityContext { merge_status: MergeStatus::Conflicts, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Medium);
        assert_eq!(s, 15);
    }

    #[test]
    fn team_review_request_via_notification() {
        let ctx = PriorityContext {
            notification_reason: Some(NotificationReason::ReviewRequested),
            is_direct_review_request: false,
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Medium);
        assert_eq!(s, 10);
    }

    #[test]
    fn comment_notification_is_medium() {
        let ctx = PriorityContext {
            notification_reason: Some(NotificationReason::Comment),
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Medium);
        assert_eq!(s, 10);
    }

    #[test]
    fn stale_alone_is_low() {
        let ctx = PriorityContext { is_stale: true, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(p, Priority::Low);
        assert_eq!(s, 5);
    }

    #[test]
    fn draft_penalty_reduces_score() {
        let ctx = PriorityContext {
            ci_status: CiStatus::Failing,
            is_draft: true,
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(s, 30); // 40 - 10
        assert_eq!(p, Priority::High); // not Critical
    }

    #[test]
    fn signals_stack() {
        let ctx = PriorityContext {
            ci_status: CiStatus::Failing,
            review_status: ReviewStatus::ChangesRequested,
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(s, 70);
        assert_eq!(p, Priority::Critical);
    }

    #[test]
    fn ci_passing_contributes_nothing() {
        let ctx = PriorityContext { ci_status: CiStatus::Passing, ..default_ctx() };
        let (_, s) = compute_priority(&ctx);
        assert_eq!(s, 0);
    }

    #[test]
    fn ci_pending_contributes_nothing() {
        let ctx = PriorityContext { ci_status: CiStatus::Pending, ..default_ctx() };
        let (_, s) = compute_priority(&ctx);
        assert_eq!(s, 0);
    }

    #[test]
    fn direct_review_request_via_notification() {
        let ctx = PriorityContext {
            notification_reason: Some(NotificationReason::ReviewRequested),
            is_direct_review_request: true,
            ..default_ctx()
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(s, 20);
        assert_eq!(p, Priority::Medium);
    }

    #[test]
    fn all_signals_stacked() {
        let ctx = PriorityContext {
            ci_status: CiStatus::Failing,
            review_status: ReviewStatus::ChangesRequested,
            merge_status: MergeStatus::Conflicts,
            is_stale: true,
            is_direct_review_request: true,
            notification_reason: Some(NotificationReason::Mention),
            is_draft: false,
        };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(s, 115); // 40+30+25+15+5
        assert_eq!(p, Priority::Critical);
    }

    #[test]
    fn draft_penalty_can_go_negative() {
        let ctx = PriorityContext { is_draft: true, ..default_ctx() };
        let (p, s) = compute_priority(&ctx);
        assert_eq!(s, -10);
        assert_eq!(p, Priority::Low);
    }
}
