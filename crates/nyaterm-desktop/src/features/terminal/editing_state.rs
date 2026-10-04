//! Session-owned shell editing state, independent of suggestion visibility.

use std::collections::HashMap;

use nyaterm_core::terminal::editing::{EditCapability, ShellEditState};
use nyaterm_core::terminal::input_tracker::TerminalInputState;
use nyaterm_terminal::editing::InputMapping;
use nyaterm_terminal::{ShellInputAnchor, TerminalLineId, TerminalSnapshot};

use crate::models::TerminalSelection;

#[derive(Default)]
pub(super) struct TerminalEditingState {
    active: String,
    pub(super) sessions: HashMap<String, SessionEditingState>,
}

#[derive(Default)]
pub(super) struct SessionEditingState {
    pub(super) command_navigation_interactive: bool,
    pub(super) model: ShellEditState,
    pub(super) capability: EditCapability,
    pub(super) mapping: Option<InputMapping>,
    pub(super) selection: Option<(TerminalSelection, u64)>,
    pub(super) selection_origin_version: Option<u64>,
    pub(super) geometry: Option<(usize, usize, u64)>,
    pub(super) observed_anchor: Option<Option<ShellInputAnchor>>,
    pub(super) submitted_anchor: Option<ShellInputAnchor>,
    pub(super) confirmation_task: Option<gpui::Task<()>>,
    pub(super) pending_evidence: Option<SnapshotEvidence>,
    pub(super) awaiting_snapshot: bool,
}

#[derive(PartialEq, Eq)]
pub(super) struct SnapshotEvidence {
    cursor: (usize, usize),
    anchor: Option<ShellInputAnchor>,
    rows: Vec<(Option<TerminalLineId>, u64)>,
}

impl SnapshotEvidence {
    pub(super) fn from_snapshot(snapshot: &TerminalSnapshot) -> Self {
        Self {
            cursor: (snapshot.cursor.row, snapshot.cursor.col),
            anchor: snapshot.shell_input_anchor,
            rows: snapshot
                .rows()
                .iter()
                .map(|row| (row.line_id, row.revision))
                .collect(),
        }
    }
}

impl SessionEditingState {
    pub(super) fn invalidate(&mut self) {
        self.model.invalidate();
        self.capability = EditCapability::None;
        self.mapping = None;
        self.selection = None;
        self.selection_origin_version = None;
        self.pending_evidence = None;
        self.confirmation_task = None;
    }
}

impl TerminalEditingState {
    pub(super) fn activate(&mut self, session_id: &str) {
        if self.active != session_id {
            if let Some(state) = self.sessions.get_mut(&self.active) {
                state.invalidate();
            }
            self.active = session_id.to_string();
            self.state_mut().invalidate();
        }
    }

    pub(super) fn state(&self) -> Option<&SessionEditingState> {
        self.sessions.get(&self.active)
    }
    pub(super) fn state_mut(&mut self) -> &mut SessionEditingState {
        self.sessions.entry(self.active.clone()).or_default()
    }

    pub(super) fn input(&self) -> &TerminalInputState {
        static EMPTY: std::sync::OnceLock<TerminalInputState> = std::sync::OnceLock::new();
        self.state().map_or_else(
            || EMPTY.get_or_init(TerminalInputState::new),
            |state| &state.model.input,
        )
    }

    /// Compatibility adapter for command suggestion replacement. Invalidates any
    /// old mapping before handing out mutable input, so it cannot authorize edits.
    pub(super) fn input_mut(&mut self) -> &mut TerminalInputState {
        let state = self.state_mut();
        state.invalidate();
        &mut state.model.input
    }

    pub(super) fn clear_active(&mut self) {
        let awaiting_snapshot = self.state_mut().awaiting_snapshot;
        *self.state_mut() = SessionEditingState {
            awaiting_snapshot,
            ..SessionEditingState::default()
        };
    }
}

impl super::state::TerminalFeatureState {
    pub(in crate::features) fn activate_shell_editing_session(&mut self, session_id: &str) {
        self.editing.activate(session_id);
    }

    pub(in crate::features) fn invalidate_shell_editing_session(&mut self, session_id: &str) {
        if let Some(state) = self.editing.sessions.get_mut(session_id) {
            state.invalidate();
            state.model.input.desynced = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalEditingState;
    use nyaterm_core::terminal::editing::EditPhase;
    use std::time::Instant;

    #[test]
    fn switching_sessions_keeps_one_input_per_session_and_cancels_pending_navigation() {
        let mut state = TerminalEditingState::default();
        state.activate("one");
        state.state_mut().model.note_input("abc", Instant::now());
        state.state_mut().model.queued_cursor = Some(1);
        state.activate("two");
        state.state_mut().model.note_input("xyz", Instant::now());
        state.activate("one");
        assert_eq!(state.input().value, "abc");
        assert_eq!(state.state().unwrap().model.phase, EditPhase::Uncertain);
        assert_eq!(state.state().unwrap().model.queued_cursor, None);
        assert_eq!(state.sessions["two"].model.input.value, "xyz");
    }
}
