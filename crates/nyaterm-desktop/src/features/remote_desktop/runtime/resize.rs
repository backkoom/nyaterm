use crate::features::NyaTermApp;
use gpui::Bounds;
use nyaterm_remote_desktop::RdpDisplayMetrics;
use nyaterm_remote_desktop::RdpDisplayMode;
use nyaterm_remote_desktop::RdpSessionState;
use std::time::Duration;
use std::time::Instant;

pub(super) const RESIZE_MIN_DELTA: u32 = 32;

pub(super) const RESIZE_FAILURE_WINDOW: Duration = Duration::from_secs(3);

pub(super) const RESIZE_DEBOUNCE: Duration = Duration::from_millis(150);

impl NyaTermApp {
    pub(in crate::features) fn update_rdp_viewport(
        &mut self,
        session_id: &str,
        bounds: Bounds<gpui::Pixels>,
        scale_factor: f32,
    ) {
        let fit_window = self.session.metadata(session_id).is_some_and(|metadata| {
            matches!(
                &metadata.launch_config,
                crate::models::SessionLaunchConfig::Rdp(config)
                    if config.display.mode == RdpDisplayMode::FitWindow
            )
        });
        let Some(session) = self.remote_desktop.sessions.get_mut(session_id) else {
            return;
        };
        session.viewport = Some(bounds);
        if !fit_window || session.dynamic_resize_disabled {
            session.pending_resize = None;
            return;
        }
        self.queue_rdp_resize(session_id, fit_window_display_metrics(bounds, scale_factor));
    }

    pub(in crate::features) fn queue_rdp_resize(
        &mut self,
        session_id: &str,
        mut metrics: RdpDisplayMetrics,
    ) {
        metrics.width = metrics.width.clamp(200, 8192) & !1;
        metrics.height = metrics.height.clamp(200, 8192) & !1;
        metrics.desktop_scale_factor = metrics.desktop_scale_factor.clamp(100, 500);
        if let Some(session) = self.remote_desktop.sessions.get_mut(session_id) {
            let remote_size = session
                .framebuffer
                .as_ref()
                .map(|framebuffer| (framebuffer.width(), framebuffer.height()));
            if session.dynamic_resize_disabled
                || !rdp_resize_is_material(remote_size, session.last_resize, metrics)
            {
                return;
            }
            session.pending_resize = Some((metrics, Instant::now()));
        }
    }

    pub(super) fn drive_rdp_resize_debounce(&mut self) -> bool {
        let now = Instant::now();
        let mut sent = false;
        for (session_id, session) in &mut self.remote_desktop.sessions {
            let Some((metrics, queued_at)) = session.pending_resize else {
                continue;
            };
            if now.saturating_duration_since(queued_at) < RESIZE_DEBOUNCE {
                continue;
            }
            session.pending_resize = None;
            if self
                .remote_desktop
                .manager
                .resize_with_metrics(session_id, metrics)
                .is_ok()
            {
                session.last_resize = Some(metrics);
                session.last_resize_sent_at = Some(now);
                sent = true;
            }
        }
        sent
    }
}

pub(super) fn rdp_resize_is_material(
    remote_size: Option<(u32, u32)>,
    last_resize: Option<RdpDisplayMetrics>,
    requested: RdpDisplayMetrics,
) -> bool {
    if last_resize == Some(requested) {
        return false;
    }
    if last_resize.is_some_and(|last| {
        last.desktop_scale_factor != requested.desktop_scale_factor
            || last.physical_size_mm != requested.physical_size_mm
    }) || (last_resize.is_none() && requested.desktop_scale_factor != 100)
    {
        return true;
    }
    let Some((remote_width, remote_height)) = remote_size else {
        return true;
    };
    remote_width.abs_diff(requested.width) >= RESIZE_MIN_DELTA
        || remote_height.abs_diff(requested.height) >= RESIZE_MIN_DELTA
}

pub(super) fn fit_window_display_metrics(
    bounds: Bounds<gpui::Pixels>,
    scale_factor: f32,
) -> RdpDisplayMetrics {
    let scale_factor = if scale_factor.is_finite() {
        scale_factor.max(1.0)
    } else {
        1.0
    };
    RdpDisplayMetrics {
        width: (f32::from(bounds.size.width) * scale_factor)
            .round()
            .max(1.0) as u32,
        height: (f32::from(bounds.size.height) * scale_factor)
            .round()
            .max(1.0) as u32,
        desktop_scale_factor: (scale_factor * 100.0).round().clamp(100.0, 500.0) as u32,
        physical_size_mm: None,
    }
}

pub(super) fn should_disable_dynamic_resize_after_state(
    state: &RdpSessionState,
    last_resize_sent_at: Option<Instant>,
    now: Instant,
) -> bool {
    matches!(
        state,
        RdpSessionState::Reconnecting | RdpSessionState::Failed(_)
    ) && last_resize_sent_at
        .is_some_and(|sent_at| now.saturating_duration_since(sent_at) <= RESIZE_FAILURE_WINDOW)
}
