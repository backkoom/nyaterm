use super::{AutoSyncCoordinator, auto_sync_backoff, permanent_auto_sync_error};
use crate::features::{AutoSyncResult, AutoSyncTrigger};
use nyaterm_core::{CloudSyncError, CloudSyncState};
use std::time::Duration;

#[test]
fn auto_sync_focus_is_throttled_and_settings_resume_paused_checks() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.pending = None;
    sync.focused();
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Focus));
    sync.pending = None;
    sync.focused();
    assert_eq!(sync.pending, None);
    sync.paused = true;
    sync.failures = 3;
    sync.settings_changed();
    assert!(!sync.paused);
    assert_eq!(sync.failures, 0);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Settings));
}

#[test]
fn auto_sync_backoff_and_auth_pause_policy() {
    assert_eq!(auto_sync_backoff(0), Duration::from_secs(60));
    assert_eq!(auto_sync_backoff(1), Duration::from_secs(300));
    assert_eq!(auto_sync_backoff(2), Duration::from_secs(900));
    assert_eq!(auto_sync_backoff(3), Duration::from_secs(3_600));
    assert_eq!(auto_sync_backoff(99), Duration::from_secs(3_600));
    assert!(permanent_auto_sync_error(
        &nyaterm_core::CloudSyncError::Remote("HTTP 401 Unauthorized".into())
    ));
    assert!(!permanent_auto_sync_error(
        &nyaterm_core::CloudSyncError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "network timeout"
        ))
    ));
}

#[test]
fn completing_an_older_sync_preserves_changes_received_while_running() {
    let mut sync = AutoSyncCoordinator::new(4);
    sync.running = true;
    sync.pending = None;
    sync.record_sync_mutation(5);
    let due = sync.next_due;
    let changed = sync.change_since;

    sync.complete(
        AutoSyncTrigger::Change,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
    );

    assert!(!sync.running());
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
    assert_eq!(sync.change_since, changed);
    assert_eq!(sync.next_due, due);

    sync.in_flight_generation = 5;
    sync.running = true;
    sync.complete(
        AutoSyncTrigger::Change,
        &Ok(AutoSyncResult::UpToDate(CloudSyncState::default())),
    );
    assert_eq!(sync.pending, None);
    assert_eq!(sync.change_since, None);
}

#[test]
fn duplicate_or_stale_mutations_do_not_extend_the_debounce() {
    let mut sync = AutoSyncCoordinator::new(4);
    sync.record_sync_mutation(5);
    let due = sync.next_due;
    let changed = sync.change_since;
    sync.record_sync_mutation(5);
    sync.record_sync_mutation(3);
    assert_eq!(sync.mutation_generation, 5);
    assert_eq!(sync.next_due, due);
    assert_eq!(sync.change_since, changed);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Change));
}

#[test]
fn authentication_failure_pauses_sync_until_settings_change() {
    let mut sync = AutoSyncCoordinator::new(0);
    sync.running = true;
    sync.complete(
        AutoSyncTrigger::Startup,
        &Err(CloudSyncError::Remote("HTTP 401 Unauthorized".into())),
    );
    assert!(!sync.running());
    assert!(sync.paused);
    assert_eq!(sync.failures, 1);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Retry));

    sync.settings_changed();
    assert!(!sync.paused);
    assert_eq!(sync.failures, 0);
    assert_eq!(sync.pending, Some(AutoSyncTrigger::Settings));
}
