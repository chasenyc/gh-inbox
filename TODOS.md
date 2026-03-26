# TODOs

## Config file support (.gh-inbox.toml)
**Priority:** Medium
**What:** Add optional `.gh-inbox.toml` config file for custom priority weights, thresholds, and notification filter settings.
**Why:** Different teams have different workflows. A security engineer might want CI failures to score higher; an OSS maintainer might want mentions scored lower. Hardcoded weights will eventually frustrate power users.
**Context:** Deferred from the Notification Center + Priority Engine design (2026-03-20) to validate the scoring model with hardcoded weights first. Implement after getting user feedback on whether the default weights feel right. Adds `toml` crate dependency, config validation code, XDG path resolution (~100 lines).
**Depends on:** Priority scoring module (priority.rs) must be implemented first.

## Auto-refresh / background polling
**Priority:** Medium
**What:** Add configurable auto-refresh that polls notifications on an interval (e.g., every 60 seconds) without pressing `r`.
**Why:** Notifications are time-sensitive. If you're waiting for a CI result or a review approval, you want it to appear automatically.
**Context:** Deferred from the Notification Center design (2026-03-20). The `If-Modified-Since` infrastructure (stored `Last-Modified` header) will already be in place from v1. Requires integrating a timer with the existing event loop and careful debouncing (don't poll while user is mid-action).
**Depends on:** Notification fetch must be implemented first.
