use crate::features::remote::job_state::{RemoteJobState, RemoteJobTicket};
use crate::features::runtime_jobs::StatsJobResult;
use futures::channel::mpsc::UnboundedReceiver;
use nyaterm_transport::{CpuUsageSource, RemoteStats, RemoteStatsSampler};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

pub(in crate::features) enum StatsApplyOutcome {
    Ignored,
    CompletedInactive,
    Applied {
        session_id: String,
        stats: Box<RemoteStats>,
        status: String,
    },
    Failed {
        status: String,
    },
}

pub(super) struct StatsPaneState {
    sampler: Arc<RemoteStatsSampler>,
    job: RemoteJobState<StatsJobResult>,
    status: String,
    cpu_expanded: bool,
    manual_job_id: Option<u64>,
    warmup_retry_pending: bool,
    warmup_retry_attempted: bool,
    /// Bumped by every mutation that changes what `stats_presentation` returns.
    revision: u64,
    active_session_id: Option<String>,
    network_history: HashMap<String, VecDeque<NetworkHistorySample>>,
    snapshots: HashMap<String, RemoteStats>,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::features) struct NetworkHistorySample {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub interfaces: HashMap<String, (f64, f64)>,
}
#[derive(Clone)]
pub(in crate::features) struct StatsPresentationState {
    pub data: Option<RemoteStats>,
    pub network_history: Arc<[NetworkHistorySample]>,
    pub cpu_expanded: bool,
    pub pending: bool,
    pub error: bool,
    pub consecutive_refresh_failures: u8,
}

impl StatsPaneState {
    /// Record that the presentation changed.
    fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn apply_data(&mut self, session_id: &str, stats: RemoteStats) {
        if stats.cpu.usage_source == CpuUsageSource::WarmingUp {
            self.warmup_retry_pending = !self.warmup_retry_attempted;
        } else {
            self.warmup_retry_pending = false;
            self.warmup_retry_attempted = false;
        }
        self.activate_session(session_id);
        self.record_network_sample(session_id, &stats);
        self.snapshots.insert(session_id.to_string(), stats);
        self.touch();
    }

    fn record_network_sample(&mut self, session_id: &str, stats: &RemoteStats) {
        let sample = NetworkHistorySample {
            rx_bytes_per_sec: stats.network_summary.rx_bytes_per_sec,
            tx_bytes_per_sec: stats.network_summary.tx_bytes_per_sec,
            interfaces: stats
                .networks
                .iter()
                .map(|network| {
                    (
                        network.nic.clone(),
                        (network.rx_bytes_per_sec, network.tx_bytes_per_sec),
                    )
                })
                .collect(),
        };
        let history = self
            .network_history
            .entry(session_id.to_string())
            .or_default();
        history.push_back(sample);
        while history.len() > 60 {
            history.pop_front();
        }
        self.touch();
    }

    pub(super) fn activate_session(&mut self, session_id: &str) {
        self.active_session_id = Some(session_id.to_string());
        self.touch();
    }

    fn active_network_history(&self) -> Arc<[NetworkHistorySample]> {
        self.active_session_id
            .as_deref()
            .and_then(|session_id| self.network_history.get(session_id))
            .map(|history| Arc::from(history.iter().cloned().collect::<Vec<_>>()))
            .unwrap_or_else(|| Arc::from([]))
    }

    fn clear_data(&mut self) {
        if let Some(session_id) = self.active_session_id.as_deref() {
            self.snapshots.remove(session_id);
        }
        self.touch();
    }

    pub(super) fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.touch();
    }

    pub(super) fn is_pending(&self) -> bool {
        self.job.is_pending()
    }

    pub(super) fn last_refresh_at(&self) -> Option<Instant> {
        self.job.last_refresh_at()
    }

    pub(super) fn consecutive_refresh_failures(&self) -> u8 {
        self.job.consecutive_refresh_failures()
    }

    pub(super) fn is_pending_for(&self, session_id: &str) -> bool {
        self.job.is_pending_for(session_id)
    }

    pub(super) fn begin_job(
        &mut self,
        session_id: String,
        manual: bool,
    ) -> RemoteJobTicket<StatsJobResult> {
        self.touch();
        let ticket = self.job.begin(session_id);
        if manual {
            self.manual_job_id = Some(ticket.job_id);
        }
        ticket
    }

    pub(super) fn mark_refresh_started(&mut self) {
        self.job.mark_refresh_started();
    }

    pub(super) fn take_event_receiver(&mut self) -> Option<UnboundedReceiver<StatsJobResult>> {
        self.job.take_event_receiver()
    }

    pub(super) fn complete_event(&mut self, job_id: u64, session_id: &str) -> bool {
        let matched = self.job.complete_if_matches(job_id, session_id);
        if matched {
            if self.manual_job_id == Some(job_id) {
                self.manual_job_id = None;
            }
            self.touch();
        }
        matched
    }

    pub(super) fn take_warmup_retry_due(&mut self) -> bool {
        if !self.warmup_retry_pending
            || self
                .last_refresh_at()
                .is_none_or(|started| started.elapsed() < std::time::Duration::from_secs(1))
        {
            return false;
        }
        self.warmup_retry_pending = false;
        self.warmup_retry_attempted = true;
        self.touch();
        true
    }

    pub(super) fn reset_refresh_failures(&mut self) {
        self.job.reset_refresh_failures();
        self.touch();
    }

    pub(super) fn record_refresh_failure(&mut self) -> u8 {
        self.touch();
        self.job.record_refresh_failure(false)
    }

    pub(super) fn toggle_cpu_expanded(&mut self) {
        self.cpu_expanded = !self.cpu_expanded;
        self.status = if self.cpu_expanded {
            "showing per-core CPU usage".to_string()
        } else {
            "collapsed per-core CPU usage".to_string()
        };
        self.touch();
    }

    pub(super) fn reset_for_session_switch(&mut self) {
        self.job.reset_for_session_switch();
        self.manual_job_id = None;
        self.warmup_retry_pending = false;
        self.warmup_retry_attempted = false;
        self.active_session_id = None;
        // Through the touching methods, not the fields: a session switch changes the
        // presentation, so it has to move the revision like any other mutation.
        self.clear_data();
        self.set_status("start an SSH session to inspect remote stats");
    }
}

impl StatsPaneState {
    pub(super) fn new() -> Self {
        Self {
            job: RemoteJobState::new(),
            status: "start an SSH session to inspect remote stats".to_string(),
            cpu_expanded: false,
            manual_job_id: None,
            warmup_retry_pending: false,
            warmup_retry_attempted: false,
            revision: 0,
            active_session_id: None,
            network_history: HashMap::new(),
            snapshots: HashMap::new(),
            sampler: Arc::new(RemoteStatsSampler::default()),
        }
    }
}

impl StatsPaneState {
    pub(super) fn stats_sampler(&self) -> Arc<RemoteStatsSampler> {
        self.sampler.clone()
    }

    pub(super) fn clear_stats_sample(&mut self, session_id: &str) {
        self.sampler.clear_session(session_id);
        self.network_history.remove(session_id);
        self.snapshots.remove(session_id);
        if self.job.is_pending_for(session_id) {
            self.job.reset_for_session_switch();
        }
        if self.active_session_id.as_deref() == Some(session_id) {
            self.clear_data();
        }
    }

    pub(super) fn stats_presentation(&self) -> StatsPresentationState {
        StatsPresentationState {
            data: self
                .active_session_id
                .as_deref()
                .and_then(|id| self.snapshots.get(id))
                .cloned(),
            network_history: self.active_network_history(),
            cpu_expanded: self.cpu_expanded,
            pending: self.is_pending(),
            error: self.consecutive_refresh_failures() > 0,
            consecutive_refresh_failures: self.consecutive_refresh_failures(),
        }
    }

    pub(super) fn stats_status(&self) -> &str {
        &self.status
    }

    pub(super) fn stats_manual_refreshing(&self) -> bool {
        self.manual_job_id.is_some()
    }

    #[cfg(test)]
    pub(super) fn record_stats_refresh_failure(&mut self) -> u8 {
        let failures = self.record_refresh_failure();
        if failures >= 3 {
            self.clear_data();
        }
        failures
    }

    pub(super) fn apply_stats_event(
        &mut self,
        event: StatsJobResult,
        active_session_id: Option<&str>,
    ) -> StatsApplyOutcome {
        if !self.complete_event(event.job_id, &event.session_id) {
            return StatsApplyOutcome::Ignored;
        }
        if active_session_id != Some(event.session_id.as_str()) {
            if let Ok(stats) = &event.result {
                self.record_network_sample(&event.session_id, stats);
                self.snapshots
                    .insert(event.session_id.clone(), stats.clone());
            }
            return StatsApplyOutcome::CompletedInactive;
        }
        match event.result {
            Ok(stats) => {
                self.reset_refresh_failures();
                let status = format!(
                    "loaded stats for {} 路 load {:.2}/{:.2}/{:.2}",
                    if stats.system.hostname.trim().is_empty() {
                        "remote host"
                    } else {
                        stats.system.hostname.as_str()
                    },
                    stats.load.load1,
                    stats.load.load5,
                    stats.load.load15
                );
                self.set_status(status.clone());
                self.apply_data(&event.session_id, stats.clone());
                StatsApplyOutcome::Applied {
                    session_id: event.session_id,
                    stats: Box::new(stats),
                    status,
                }
            }
            Err(error) => {
                let failures = self.record_refresh_failure();
                if failures >= 3 {
                    self.clear_data();
                }
                let status = format!("stats refresh failed: {error}");
                self.set_status(status.clone());
                StatsApplyOutcome::Failed { status }
            }
        }
    }
}
