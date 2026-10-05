use super::DesktopController;
use crate::features::AutoSyncResult;
use crate::features::AutoSyncTrigger;
use crate::features::run_auto_sync;
use futures::StreamExt as _;
use futures::future::Either;
use futures::future::select;
use gpui::Context;
use nyaterm_core::WorkspaceId;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

pub(super) struct AutoSyncCoordinator {
    running: bool,
    pending: Option<AutoSyncTrigger>,
    next_due: Instant,
    next_periodic: Instant,
    last_focus: Option<Instant>,
    mutation_generation: u64,
    in_flight_generation: u64,
    change_since: Option<Instant>,
    remote_available: bool,
    failures: usize,
    paused: bool,
}

impl AutoSyncCoordinator {
    fn complete(
        &mut self,
        trigger: AutoSyncTrigger,
        result: &Result<AutoSyncResult, nyaterm_core::CloudSyncError>,
    ) {
        let now = Instant::now();
        self.running = false;
        match result {
            Ok(AutoSyncResult::Debouncing(remaining)) => {
                self.pending = Some(AutoSyncTrigger::Change);
                self.next_due = now + *remaining;
            }
            Ok(AutoSyncResult::Busy) => {
                self.pending = Some(trigger);
                self.next_due = now + Duration::from_secs(5);
            }
            Ok(AutoSyncResult::RemoteAvailable { retry_when_safe }) => {
                self.remote_available = *retry_when_safe;
                self.failures = 0;
            }
            Ok(AutoSyncResult::Synced(_)) | Ok(AutoSyncResult::UpToDate(_)) => {
                self.remote_available = false;
                self.failures = 0;
                if self.mutation_generation == self.in_flight_generation {
                    self.change_since = None;
                    if self.pending == Some(AutoSyncTrigger::Change) {
                        self.pending = None;
                    }
                }
            }
            Err(nyaterm_core::CloudSyncError::Conflict(_)) => {
                self.remote_available = false;
            }
            Err(error) => {
                self.paused = permanent_auto_sync_error(error);
                let backoff = auto_sync_backoff(self.failures);
                self.failures = self.failures.saturating_add(1);
                self.pending = Some(AutoSyncTrigger::Retry);
                self.next_due = now + backoff;
            }
            _ => {}
        }
    }

    pub(super) fn running(&self) -> bool {
        self.running
    }

    fn record_sync_mutation(&mut self, generation: u64) {
        if generation <= self.mutation_generation {
            return;
        }
        self.mutation_generation = generation;
        let now = Instant::now();
        self.change_since = Some(now);
        self.pending = Some(AutoSyncTrigger::Change);
        self.next_due = now + Duration::from_secs(1);
    }

    pub(super) fn new(mutation_generation: u64) -> Self {
        let now = Instant::now();
        Self {
            running: false,
            pending: Some(AutoSyncTrigger::Startup),
            next_due: now + Duration::from_secs(3),
            next_periodic: now + Duration::from_secs(15 * 60),
            last_focus: None,
            mutation_generation,
            in_flight_generation: mutation_generation,
            change_since: None,
            remote_available: false,
            failures: 0,
            paused: false,
        }
    }

    pub(super) fn settings_changed(&mut self) {
        self.paused = false;
        self.failures = 0;
        if self.pending != Some(AutoSyncTrigger::Change) {
            self.pending = Some(AutoSyncTrigger::Settings);
            self.next_due = Instant::now();
        }
    }

    pub(super) fn focused(&mut self) {
        let now = Instant::now();
        if self
            .last_focus
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(120))
        {
            self.last_focus = Some(now);
            if self.pending.is_none() {
                self.pending = Some(AutoSyncTrigger::Focus);
                self.next_due = now;
            }
        }
    }
}

pub(super) fn permanent_auto_sync_error(error: &nyaterm_core::CloudSyncError) -> bool {
    match error {
        nyaterm_core::CloudSyncError::LocalStore(message)
            if message == "crypto" || message == "invalid_data" =>
        {
            true
        }
        nyaterm_core::CloudSyncError::Disabled
        | nyaterm_core::CloudSyncError::PortableSnapshot(
            nyaterm_core::PortableSnapshotError::MissingMasterPassword
            | nyaterm_core::PortableSnapshotError::Decrypt { .. },
        ) => true,
        nyaterm_core::CloudSyncError::Remote(message) => {
            let message = message.to_ascii_lowercase();
            [
                "401",
                "403",
                "unauthorized",
                "forbidden",
                "invalid token",
                "missing token",
                "access token is required",
                "endpoint is required",
                "bucket is required",
                "credential is required",
            ]
            .iter()
            .any(|marker| message.contains(marker))
        }
        _ => false,
    }
}

pub(super) fn auto_sync_backoff(failures: usize) -> Duration {
    Duration::from_secs([1, 5, 15, 60][failures.min(3)] * 60)
}

impl DesktopController {
    pub(super) fn start_auto_sync_runtime(&mut self, cx: &mut Context<Self>) {
        if let Some(mut events) = self
            .startup
            .shared_store_runtime()
            .and_then(|store| store.take_sync_mutation_events())
        {
            cx.spawn(async move |this, cx| {
                while let Some(event) = events.next().await {
                    if this
                        .update(cx, |controller, _| {
                            controller.auto_sync.record_sync_mutation(event.generation)
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |controller, cx| controller.tick_auto_sync(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn tick_auto_sync(&mut self, cx: &mut Context<Self>) {
        if self.process_quitting || self.auto_sync.running {
            return;
        }
        let Some(runtime_store) = self.startup.shared_store_runtime() else {
            return;
        };
        let now = Instant::now();
        let generation = runtime_store.sync_mutation_generation();
        if generation != self.auto_sync.mutation_generation {
            self.auto_sync.record_sync_mutation(generation);
        }
        if now >= self.auto_sync.next_periodic {
            self.auto_sync.next_periodic = now + Duration::from_secs(15 * 60);
            if self.auto_sync.pending.is_none() {
                self.auto_sync.pending = Some(AutoSyncTrigger::Periodic);
                self.auto_sync.next_due = now;
            }
        }
        if self.auto_sync.remote_available
            && self.auto_sync.pending.is_none()
            && !self.auto_sync.paused
            && self.auto_sync_pull_safe(cx)
        {
            self.auto_sync.pending = Some(AutoSyncTrigger::DeferredPull);
            self.auto_sync.next_due = now;
        }
        if self.auto_sync.paused || now < self.auto_sync.next_due {
            return;
        }
        let Some(trigger) = self.auto_sync.pending.take() else {
            return;
        };
        let change_elapsed = self
            .auto_sync
            .change_since
            .map_or(Duration::ZERO, |since| now.duration_since(since));
        let pull_safe = self.auto_sync_pull_safe(cx);
        self.auto_sync.running = true;
        self.auto_sync.in_flight_generation = self.auto_sync.mutation_generation;
        let store = runtime_store.blocking_client();
        let runtime = self.runtime.clone();
        let scheduler = self.update_store.read(cx).blocking_jobs();
        let (guard_request, guard_receiver) =
            futures::channel::oneshot::channel::<std::sync::mpsc::SyncSender<bool>>();
        let guard_request = Mutex::new(Some(guard_request));
        let expired = Arc::new(AtomicBool::new(false));
        let task_expired = expired.clone();
        let task = scheduler.submit_task("cloud-sync-auto", move |_| {
            let before_apply = || {
                if task_expired.load(Ordering::Acquire) {
                    return Err(nyaterm_core::CloudSyncError::AutoPullDeferred);
                }
                let (reply, receiver) = std::sync::mpsc::sync_channel(1);
                guard_request
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .ok_or(nyaterm_core::CloudSyncError::AutoPullDeferred)?
                    .send(reply)
                    .map_err(|_| nyaterm_core::CloudSyncError::AutoPullDeferred)?;
                if receiver
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap_or(false)
                    && !task_expired.load(Ordering::Acquire)
                {
                    Ok(())
                } else {
                    Err(nyaterm_core::CloudSyncError::AutoPullDeferred)
                }
            };
            run_auto_sync(
                &store,
                &runtime,
                trigger,
                change_elapsed,
                pull_safe,
                &before_apply,
            )
        });
        let guard_expired = expired.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(reply) = guard_receiver.await {
                let safe = !guard_expired.load(Ordering::Acquire)
                    && this
                        .update(cx, |controller, cx| {
                            !controller.process_quitting && controller.auto_sync_pull_safe(cx)
                        })
                        .unwrap_or(false);
                let _ = reply.send(safe);
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let result = match task {
                Ok(task) => {
                    let timeout = cx.background_executor().timer(Duration::from_secs(300));
                    match select(Box::pin(task), Box::pin(timeout)).await {
                        Either::Left((result, _)) => result.unwrap_or_else(|error| {
                            Err(nyaterm_core::CloudSyncError::Remote(error.to_string()))
                        }),
                        Either::Right((_, task)) => {
                            expired.store(true, Ordering::Release);
                            task.cancel();
                            Err(nyaterm_core::CloudSyncError::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "automatic cloud sync exceeded 300 seconds",
                            )))
                        }
                    }
                }
                Err(error) => Err(nyaterm_core::CloudSyncError::Remote(error.to_string())),
            };
            let _ = this.update(cx, |controller, cx| {
                controller.complete_auto_sync(trigger, result, cx);
            });
        })
        .detach();
    }

    pub(super) fn auto_sync_pull_safe(&self, cx: &gpui::App) -> bool {
        self.windows.values().all(|entry| {
            entry
                .shell
                .upgrade()
                .and_then(|shell| shell.read(cx).app.clone())
                .is_some_and(|app| !app.read(cx).auto_cloud_sync_pull_blocked())
        })
    }

    pub(super) fn complete_auto_sync(
        &mut self,
        trigger: AutoSyncTrigger,
        result: Result<AutoSyncResult, nyaterm_core::CloudSyncError>,
        cx: &mut Context<Self>,
    ) {
        self.auto_sync.complete(trigger, &result);
        for entry in self.windows.values() {
            let _ = entry.shell.update(cx, |shell, cx| {
                if let Some(app) = shell.app.as_ref() {
                    app.update(cx, |app, cx| app.apply_auto_cloud_sync_result(&result, cx));
                }
            });
        }
    }

    pub(crate) fn cloud_sync_pull_blocked_excluding(
        &self,
        excluded: WorkspaceId,
        cx: &gpui::App,
    ) -> bool {
        self.windows
            .iter()
            .filter(|(id, _)| **id != excluded)
            .any(|(_, entry)| {
                entry
                    .shell
                    .upgrade()
                    .and_then(|shell| shell.read(cx).app.clone())
                    .is_none_or(|app| app.read(cx).cloud_sync_session_restore_blocked())
            })
    }

    pub(crate) fn cloud_sync_job_running_excluding(
        &self,
        excluded: WorkspaceId,
        cx: &gpui::App,
    ) -> bool {
        self.auto_sync.running
            || self
                .windows
                .iter()
                .filter(|(id, _)| **id != excluded)
                .any(|(_, entry)| {
                    entry
                        .shell
                        .upgrade()
                        .and_then(|shell| shell.read(cx).app.clone())
                        .is_some_and(|app| app.read(cx).cloud_sync_job_running())
                })
    }
}

#[cfg(test)]
mod tests;
