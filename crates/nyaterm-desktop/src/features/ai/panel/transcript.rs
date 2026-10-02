use crate::features::ai::presentation::AiAgentStepKind;
use std::collections::HashSet;
use std::ops::Range;

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};
use rust_i18n::t;

use crate::features::ai::panel::{AiPanel, AiPanelSnapshot};

/// Stable identity and a lookup into the immutable panel snapshot. Domain data
/// remains owned by AI state; these rows only describe the visible timeline.
#[derive(Clone)]
pub(super) enum AiTranscriptRow {
    Empty,
    Message { index: usize, id: String },
    AgentHeader,
    AgentStep { index: usize, step_index: u16 },
    Command { index: usize, id: String },
}

impl AiTranscriptRow {
    pub(super) fn project(snapshot: &AiPanelSnapshot) -> Vec<Self> {
        let mut rows = Vec::new();
        if snapshot.messages.is_empty() {
            rows.push(Self::Empty);
        } else {
            rows.extend(
                snapshot
                    .messages
                    .iter()
                    .enumerate()
                    .map(|(index, message)| Self::Message {
                        index,
                        id: message.id.clone(),
                    }),
            );
        }
        let steps: Vec<_> = snapshot
            .agent_steps
            .iter()
            .enumerate()
            .skip(snapshot.agent_steps.len().saturating_sub(16))
            .filter(|(_, presentation)| {
                let step = &presentation.step;
                let linked_command = step
                    .command_card_id
                    .as_deref()
                    .is_some_and(|id| snapshot.command_step(id).is_some());
                let linked_answer = step.kind == AiAgentStepKind::FinalAnswer
                    && step.source_message_id.as_ref().is_some_and(|id| {
                        snapshot.messages.iter().any(|message| &message.id == id)
                    });
                !linked_command && !linked_answer
            })
            .collect();
        if !steps.is_empty() {
            rows.push(Self::AgentHeader);
            rows.extend(
                steps
                    .into_iter()
                    .map(|(index, presentation)| Self::AgentStep {
                        index,
                        step_index: presentation.step.step_index,
                    }),
            );
        }
        let mut shown = HashSet::new();
        rows.extend(
            snapshot
                .command_cards
                .iter()
                .take(8)
                .enumerate()
                .filter(|(_, card)| {
                    snapshot.card_owner(&card.id).is_none() && shown.insert(card.id.as_str())
                })
                .map(|(index, card)| Self::Command {
                    index,
                    id: card.id.clone(),
                }),
        );
        // A truly empty conversation uses the full-height introduction outside
        // the list, so it can remain centered in the available viewport.
        if matches!(rows.as_slice(), [Self::Empty]) {
            rows.clear();
        }
        rows
    }

    fn same_item(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Message { id: a, .. }, Self::Message { id: b, .. })
            | (Self::Command { id: a, .. }, Self::Command { id: b, .. }) => a == b,
            (Self::AgentStep { step_index: a, .. }, Self::AgentStep { step_index: b, .. }) => {
                a == b
            }
            (Self::Empty, Self::Empty) | (Self::AgentHeader, Self::AgentHeader) => true,
            _ => false,
        }
    }

    fn content_changed(
        &self,
        previous: &AiPanelSnapshot,
        other: &Self,
        next: &AiPanelSnapshot,
    ) -> bool {
        match (self, other) {
            (Self::Message { index: a, id }, Self::Message { index: b, .. }) => {
                previous.messages[*a] != next.messages[*b]
                    || (previous.streaming_assistant_id.as_ref() == Some(id))
                        != (next.streaming_assistant_id.as_ref() == Some(id))
                    || (next.streaming_assistant_id.as_ref() == Some(id)
                        && previous.response_phase != next.response_phase)
                    || previous.expanded_message_thoughts.contains(id)
                        != next.expanded_message_thoughts.contains(id)
                    || next.messages[*b].command_cards.iter().any(|card| {
                        previous.command_step(&card.id) != next.command_step(&card.id)
                            || previous.card_owner(&card.id) != next.card_owner(&card.id)
                            || previous.expanded_command_details.contains(&card.id)
                                != next.expanded_command_details.contains(&card.id)
                    })
            }
            (Self::AgentStep { index: a, .. }, Self::AgentStep { index: b, .. }) => {
                let a = &previous.agent_steps[*a];
                let b = &next.agent_steps[*b];
                a != b
            }
            (Self::Command { index: a, .. }, Self::Command { index: b, .. }) => {
                let id = &next.command_cards[*b].id;
                previous.command_cards[*a] != next.command_cards[*b]
                    || previous.command_step(id) != next.command_step(id)
                    || previous.expanded_command_details.contains(id)
                        != next.expanded_command_details.contains(id)
            }
            // Setup and enabled-model changes can replace the introduction.
            (Self::Empty, Self::Empty) => {
                previous.enabled != next.enabled
                    || previous.external_agent != next.external_agent
                    || previous.selected_model_id != next.selected_model_id
                    || previous.enabled_models.is_empty() != next.enabled_models.is_empty()
            }
            _ => false,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct AiTranscriptUpdate {
    pub reset: bool,
    pub splice: Option<(Range<usize>, usize)>,
    pub remeasure_all: bool,
    pub remeasure: Vec<Range<usize>>,
}

impl AiTranscriptUpdate {
    pub(super) fn between(
        previous: Option<&AiPanelSnapshot>,
        next: &AiPanelSnapshot,
        old_rows: &[AiTranscriptRow],
        rows: &[AiTranscriptRow],
    ) -> Self {
        let Some(previous) = previous.filter(|previous| {
            previous.current_ai_session_id == next.current_ai_session_id
                && previous.owner_terminal_id == next.owner_terminal_id
                && previous.owner_connection_id == next.owner_connection_id
        }) else {
            return Self {
                reset: true,
                ..Self::default()
            };
        };
        let prefix = old_rows
            .iter()
            .zip(rows)
            .take_while(|(a, b)| a.same_item(b))
            .count();
        let suffix = old_rows[prefix..]
            .iter()
            .rev()
            .zip(rows[prefix..].iter().rev())
            .take_while(|(a, b)| a.same_item(b))
            .count();
        let old_end = old_rows.len() - suffix;
        let new_end = rows.len() - suffix;
        let mut update = Self {
            splice: (prefix != old_end || prefix != new_end)
                .then_some((prefix..old_end, new_end - prefix)),
            remeasure_all: previous.chrome.palette != next.chrome.palette
                || previous.ui_font_family != next.ui_font_family
                || previous.chrome.viewport_width != next.chrome.viewport_width
                || previous.chrome.viewport_height != next.chrome.viewport_height,
            ..Self::default()
        };
        if !update.remeasure_all {
            for (old_index, new_index) in (0..prefix)
                .map(|index| (index, index))
                .chain((0..suffix).map(|index| (old_end + index, new_end + index)))
            {
                if old_rows[old_index].content_changed(previous, &rows[new_index], next) {
                    update.remeasure.push(new_index..new_index + 1);
                }
            }
        }
        update
    }
}

impl AiPanel {
    pub(super) fn ai_transcript_row(
        &self,
        snapshot: &AiPanelSnapshot,
        row: &AiTranscriptRow,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = snapshot.chrome.palette;
        match row {
            AiTranscriptRow::Empty => self.ai_empty_transcript(snapshot, cx),
            AiTranscriptRow::Message { index, .. } => self
                .ai_message_bubble(snapshot, &snapshot.messages[*index], cx)
                .into_any_element(),
            AiTranscriptRow::AgentHeader => div()
                .mt_2()
                .border_t_1()
                .border_color(rgb(palette.border))
                .pt_2()
                .text_size(px(10.))
                .font_weight(gpui::FontWeight(700.))
                .text_color(rgb(palette.text_muted))
                .child(t!("ai.agentSteps"))
                .into_any_element(),
            AiTranscriptRow::AgentStep { index, .. } => {
                self.ai_agent_step_card(palette, snapshot.agent_steps[*index].clone(), cx)
            }
            AiTranscriptRow::Command { index, .. } => {
                self.ai_command_card_view(snapshot, snapshot.command_cards[*index].clone(), cx)
            }
        }
    }
}
