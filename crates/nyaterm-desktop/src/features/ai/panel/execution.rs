//! Request-scoped execution groups projected from the existing transcript.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use gpui::{Context, IntoElement, div, prelude::*, px, rgb};
use nyaterm_core::ai::AiMessageRole;
use nyaterm_ui::chat::running_indicator;
use rust_i18n::t;

use crate::features::ai::is_agent_command_card;
use crate::features::ai::panel::messages::disclosure;
use crate::features::ai::panel::transcript::AiTranscriptRow;
use crate::features::ai::panel::{AiPanel, AiPanelSnapshot};
use crate::features::ai::presentation::{AiAgentStepKind, AiResponsePhase};
use crate::features::formatting::extract_think_content;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AiExecutionGroup {
    pub id: String,
    pub messages: Range<usize>,
    pub final_message: Option<usize>,
    pub steps: Vec<usize>,
}

impl AiExecutionGroup {
    pub(super) fn project(snapshot: &AiPanelSnapshot) -> Vec<Self> {
        let indices: HashMap<&str, usize> = snapshot
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| (message.id.as_str(), index))
            .collect();
        let mut groups = Vec::new();
        let mut start = 0;
        let mut owner = None;
        for end in 0..=snapshot.messages.len() {
            if end != snapshot.messages.len() && snapshot.messages[end].role != AiMessageRole::User
            {
                continue;
            }
            let range = start..end;
            let steps: Vec<usize> = snapshot
                .agent_steps
                .iter()
                .enumerate()
                .filter_map(|(index, presentation)| {
                    let step = &presentation.step;
                    let source = step
                        .source_message_id
                        .as_deref()
                        .filter(|id| indices.contains_key(id))
                        .or_else(|| {
                            if snapshot.step_is_active(step.step_index) {
                                snapshot.streaming_assistant_id.as_deref()
                            } else {
                                None
                            }
                        });
                    source
                        .and_then(|id| indices.get(id))
                        .filter(|index| range.contains(*index))
                        .map(|_| index)
                })
                .collect();
            let has_execution = snapshot.messages[range.clone()].iter().any(|message| {
                message.role == AiMessageRole::Assistant
                    && message.command_cards.iter().any(|card| {
                        is_agent_command_card(card) || snapshot.command_step(&card.id).is_some()
                    })
            }) || steps.iter().any(|index| {
                snapshot.agent_steps[*index].step.kind != AiAgentStepKind::FinalAnswer
            }) || (end == snapshot.messages.len()
                && snapshot.native_run.as_ref().is_some_and(|view| {
                    !view.plan.tasks.is_empty()
                        || !view.questions.is_empty()
                        || view.verification.is_some()
                }));
            if has_execution && !range.is_empty() {
                let final_message = range
                    .clone()
                    .rev()
                    .find(|index| {
                        let message = &snapshot.messages[*index];
                        message.role == AiMessageRole::Assistant
                            && !extract_think_content(&message.content).0.trim().is_empty()
                    })
                    .filter(|index| {
                        let message = &snapshot.messages[*index];
                        if !message.command_cards.is_empty() {
                            return false;
                        }
                        let source_steps: Vec<_> = steps
                            .iter()
                            .map(|index| &snapshot.agent_steps[*index].step)
                            .filter(|step| {
                                step.source_message_id.as_deref() == Some(message.id.as_str())
                            })
                            .collect();
                        let final_step = source_steps
                            .iter()
                            .any(|step| step.kind == AiAgentStepKind::FinalAnswer);
                        let tool_step = source_steps
                            .iter()
                            .any(|step| step.kind != AiAgentStepKind::FinalAnswer);
                        let still_preparing = snapshot.running
                            && snapshot.streaming_assistant_id.as_deref()
                                == Some(message.id.as_str())
                            && snapshot.response_phase != AiResponsePhase::Responding;
                        final_step || (!tool_step && !still_preparing)
                    });
                groups.push(Self {
                    id: owner
                        .clone()
                        .unwrap_or_else(|| snapshot.messages[start].id.clone()),
                    messages: range,
                    final_message,
                    steps,
                });
            }
            if end < snapshot.messages.len() {
                owner = Some(snapshot.messages[end].id.clone());
                start = end + 1;
            }
        }
        groups
    }

    pub(super) fn is_live(&self, snapshot: &AiPanelSnapshot) -> bool {
        snapshot.running
            && (self
                .steps
                .iter()
                .any(|index| snapshot.step_is_active(snapshot.agent_steps[*index].step.step_index))
                || self.messages.clone().any(|index| {
                    snapshot.streaming_assistant_id.as_deref()
                        == Some(snapshot.messages[index].id.as_str())
                }))
    }

    pub(super) fn step_count(&self, snapshot: &AiPanelSnapshot) -> usize {
        let commands = snapshot.messages[self.messages.clone()]
            .iter()
            .flat_map(|message| &message.command_cards)
            .map(|card| card.id.as_str())
            .collect::<HashSet<_>>()
            .len();
        commands
            + self
                .steps
                .iter()
                .filter(|index| {
                    let step = &snapshot.agent_steps[**index].step;
                    step.command_card_id.is_none()
                        && matches!(
                            step.kind,
                            AiAgentStepKind::ToolProgress
                                | AiAgentStepKind::Diagnostic
                                | AiAgentStepKind::Command
                                | AiAgentStepKind::Observation
                        )
                })
                .count()
    }
}

impl AiPanel {
    pub(super) fn ai_execution_header(
        &self,
        snapshot: &AiPanelSnapshot,
        group: &AiExecutionGroup,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = group.id.clone();
        let palette = snapshot.chrome.palette;
        let live = group.is_live(snapshot);
        let open = snapshot.expanded_execution_groups.contains(&id);
        let count = group.step_count(snapshot);
        let label = if live {
            t!("ai.executionInProgress", count = count)
        } else {
            t!("ai.executionSteps", count = count)
        };
        div()
            .min_w_0()
            .w_full()
            .flex()
            .items_center()
            .gap_1()
            .pb_2()
            .border_b_1()
            .border_color(rgb(palette.border))
            .when(live && !open, |row| {
                row.child(running_indicator(
                    format!("ai-execution-running-{id}"),
                    rgb(palette.link),
                ))
            })
            .child(disclosure(
                format!("ai-execution-toggle-{id}"),
                label,
                open,
                cx.listener(move |panel, _, _, cx| {
                    let anchor = panel.transcript_rows.iter().position(
                        |row| matches!(row, AiTranscriptRow::Execution { group } if group.id == id),
                    );
                    if !open && let Some(index) = anchor {
                        // Anchor the reading position before inserting history rows.
                        panel.transcript_scroll.update(cx, |scroll, cx| {
                            scroll.scroll_to_item(index, cx);
                        });
                    }
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| {
                        app.ai.toggle_execution_group(id);
                        app.defer_ai_panel_snapshot_flush(cx);
                    });
                }),
            ))
            .text_size(px(12.))
            .into_any_element()
    }
}
