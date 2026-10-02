use std::collections::HashSet;

use gpui::{
    AnyElement, App, ClickEvent, ClipboardItem, Context, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, SharedString, Window, div, prelude::*, px, rgb,
};
use nyaterm_core::ai::{AiCommandCard, AiMessage, AiMessageRole, RiskLevel};
use nyaterm_ui::chat::{NyaShimmerText, running_indicator};
use nyaterm_ui::{NyaButton, NyaButtonVariant};
use rust_i18n::t;

use crate::features::ai::is_agent_command_card;
use crate::features::ai::panel::{AiAgentStepPresentation, AiPanel, AiPanelSnapshot};
use crate::features::ai::presentation::{AiAgentStepKind, AiCommandPhase};
use crate::features::formatting::extract_think_content;
use crate::features::shell::gpui_code_font_family;
use crate::features::view_widgets::{markdown_answer_view, markdown_content_view};
use crate::models::AiMessageMenuState;
use crate::theme::ThemePalette;

fn localized_risk(risk: Option<&RiskLevel>) -> String {
    t!(match risk {
        Some(RiskLevel::Low) => "ai.riskLow",
        Some(RiskLevel::Medium) => "ai.riskMedium",
        Some(RiskLevel::High) => "ai.riskHigh",
        Some(RiskLevel::Critical) => "ai.riskCritical",
        None => "ai.riskUnrated",
    })
    .to_string()
}

impl AiPanelSnapshot {
    pub(super) fn command_step(&self, card_id: &str) -> Option<&AiAgentStepPresentation> {
        self.agent_steps.iter().find(|presentation| {
            presentation.step.command_card_id.as_deref() == Some(card_id)
                && presentation
                    .step
                    .source_message_id
                    .as_ref()
                    .is_some_and(|id| {
                        self.messages.iter().any(|message| {
                            &message.id == id
                                && message.command_cards.iter().any(|card| card.id == card_id)
                        })
                    })
        })
    }

    pub(super) fn command_phase(&self, card: &AiCommandCard) -> AiCommandPhase {
        self.command_step(&card.id)
            .map(|presentation| AiCommandPhase::from_step(&presentation.step))
            .unwrap_or_else(|| {
                if is_agent_command_card(card) {
                    AiCommandPhase::HistoryUnknown
                } else {
                    AiCommandPhase::Suggested
                }
            })
    }

    pub(super) fn card_owner(&self, card_id: &str) -> Option<&str> {
        self.messages
            .iter()
            .find(|message| message.command_cards.iter().any(|card| card.id == card_id))
            .map(|message| message.id.as_str())
    }
}

fn disclosure(
    id: String,
    label: impl Into<SharedString>,
    open: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let selector = id.clone();
    let content_selector = format!("{id}-content");
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .justify_start()
        .items_center()
        .child(
            div()
                .flex_none()
                .debug_selector(move || content_selector.clone())
                .child(
                    NyaButton::new(id, label)
                        .small()
                        .compact()
                        .variant(NyaButtonVariant::Ghost)
                        .icon(if open {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/menu/chevron-right.svg"
                        })
                        .on_click(on_click),
                ),
        )
        .into_any_element()
}

fn secondary(
    id: String,
    label: impl Into<SharedString>,
    variant: NyaButtonVariant,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let selector = id.clone();
    div()
        .debug_selector(move || selector.clone())
        .child(
            NyaButton::new(id, label)
                .small()
                .variant(variant)
                .on_click(on_click),
        )
        .into_any_element()
}

fn thinking_indicator(id: &str) -> AnyElement {
    let selector = format!("ai-thinking-{id}");
    div()
        .debug_selector({
            let selector = selector.clone();
            move || selector.clone()
        })
        .child(
            NyaShimmerText::new(t!("ai.thinking"))
                .id(selector)
                .duration(std::time::Duration::from_secs(2)),
        )
        .into_any_element()
}

impl AiPanel {
    pub(super) fn ai_message_bubble(
        &self,
        snapshot: &AiPanelSnapshot,
        message: &AiMessage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = snapshot.chrome.palette;
        let is_user = message.role == AiMessageRole::User;
        let streaming = snapshot.streaming_assistant_id.as_deref() == Some(message.id.as_str());
        let thinking = streaming && snapshot.response_phase.shows_thinking();
        let (display, embedded_thought) = extract_think_content(&message.content);
        let reasoning = message
            .reasoning_content
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string)
            .or(embedded_thought);
        let menu_text = if display.is_empty() {
            message.content.clone()
        } else {
            display.clone()
        };
        let menu_id = message.id.clone();
        let mut body = div()
            .id(SharedString::from(format!("ai-msg-{}", message.id)))
            .debug_selector({
                let id = message.id.clone();
                move || format!("ai-message-{id}")
            })
            .min_w_0()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(12.))
            .line_height(px(18.))
            .text_color(rgb(palette.text))
            .when(is_user, |body| {
                body.rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.hover))
                    .p_2()
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |panel, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let menu = AiMessageMenuState {
                        message_id: menu_id.clone(),
                        text: menu_text.clone(),
                        x: event.position.x,
                        y: event.position.y,
                    };
                    panel.with_app(cx, move |app, _| app.ai.open_message_menu(menu));
                }),
            );
        if let Some(reasoning) = reasoning {
            let open = snapshot.expanded_message_thoughts.contains(&message.id);
            let id = message.id.clone();
            let header = disclosure(
                format!("ai-thought-toggle-{id}"),
                t!("ai.thoughtProcess"),
                open,
                cx.listener(move |panel, _, _, cx| {
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| {
                        app.ai.toggle_message_thought(id);
                        app.defer_ai_panel_snapshot_flush(cx);
                    });
                }),
            );
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(rgb(palette.text_muted))
                    .child(header)
                    .when(thinking, |header| {
                        header.child(thinking_indicator(&message.id))
                    }),
            );
            if open {
                body = body.child(
                    div()
                        .min_w_0()
                        .w_full()
                        .text_color(rgb(palette.text_muted))
                        .child(markdown_content_view(palette, &reasoning)),
                );
            }
        } else if thinking {
            body = body.child(
                div()
                    .text_color(rgb(palette.text_muted))
                    .child(thinking_indicator(&message.id)),
            );
        }
        if !display.is_empty() {
            body = body.child(if is_user {
                crate::features::ai::panel::components::ai_user_pre_wrap_text(palette, &display)
            } else {
                div()
                    .debug_selector({
                        let id = message.id.clone();
                        move || format!("ai-answer-{id}")
                    })
                    .min_w_0()
                    .w_full()
                    .child(markdown_answer_view(palette, &display))
                    .into_any_element()
            });
        }
        let mut shown = HashSet::new();
        for card in &message.command_cards {
            if shown.insert(&card.id) && snapshot.card_owner(&card.id) == Some(message.id.as_str())
            {
                body = body.child(self.ai_command_card_view(snapshot, card.clone(), cx));
            }
        }
        body.into_any_element()
    }

    pub(super) fn ai_agent_step_card(
        &self,
        palette: ThemePalette,
        presentation: AiAgentStepPresentation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = presentation.step;
        let index = step.step_index;
        let mut row = div()
            .id(format!("ai-agent-step-{index}"))
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(12.))
            .line_height(px(18.));
        // Unlinked progress stays compact; commands and answers are explicit payload kinds.
        row = row.child(
            div()
                .text_color(rgb(palette.text_muted))
                .child(step.title.clone()),
        );
        if step.kind == AiAgentStepKind::FinalAnswer {
            return row
                .child(markdown_answer_view(palette, &step.detail))
                .into_any_element();
        }
        if let Some(thought) = step.thought {
            row = row.child(disclosure(
                format!("ai-agent-thought-{index}"),
                t!("ai.thoughtProcess"),
                presentation.thought_open,
                cx.listener(move |panel, _, _, cx| {
                    panel.with_app(cx, move |app, cx| {
                        app.toggle_ai_agent_thought_expanded(index, cx)
                    })
                }),
            ));
            if presentation.thought_open {
                row = row.child(markdown_content_view(palette, &thought));
            }
        }
        if let Some(command) = step.command {
            row = row.child(
                div()
                    .font_family(gpui_code_font_family())
                    .text_color(rgb(palette.text))
                    .child(self.highlighted_command(&command)),
            );
        }
        if let Some(output) = step.observation {
            row = row.child(disclosure(
                format!("ai-agent-output-{index}"),
                t!("ai.executionOutput"),
                presentation.output_open,
                cx.listener(move |panel, _, _, cx| {
                    panel.with_app(cx, move |app, cx| {
                        app.toggle_ai_agent_output_expanded(index, cx)
                    })
                }),
            ));
            if presentation.output_open {
                row = row.child(
                    div()
                        .font_family(gpui_code_font_family())
                        .text_color(rgb(palette.text_muted))
                        .child(output),
                );
            }
        } else if matches!(
            step.kind,
            AiAgentStepKind::ToolProgress | AiAgentStepKind::Diagnostic
        ) && !step.detail.is_empty()
        {
            row = row.child(div().text_color(rgb(palette.text_muted)).child(step.detail));
        }
        row.into_any_element()
    }

    pub(super) fn ai_command_card_view(
        &self,
        snapshot: &AiPanelSnapshot,
        card: AiCommandCard,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = snapshot.chrome.palette;
        let phase = snapshot.command_phase(&card);
        let linked = snapshot.command_step(&card.id);
        let key = card.id.clone();
        let color = match phase {
            AiCommandPhase::NeedsApproval => palette.warning,
            AiCommandPhase::Failed => palette.danger,
            AiCommandPhase::Running | AiCommandPhase::Preparing => palette.link,
            AiCommandPhase::Completed => palette.success,
            _ => palette.text_muted,
        };
        let target = card
            .target
            .as_ref()
            .map(|target| target.label.as_str())
            .or(card.target_terminal_session_id.as_deref())
            .unwrap_or_default();
        let mut block = div()
            .id(format!("ai-command-card-{key}"))
            .debug_selector({
                let key = key.clone();
                move || format!("ai-command-{key}")
            })
            .min_w_0()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.surface))
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(12.))
            .line_height(px(18.))
            .text_color(rgb(palette.text))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_weight(FontWeight(600.))
                            .child(card.title.clone()),
                    )
                    .child(
                        div()
                            .text_color(rgb(color))
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .rounded_sm()
                            .bg(rgb(color).opacity(0.08))
                            .when(phase == AiCommandPhase::Running, |status| {
                                status.child(
                                    div()
                                        .debug_selector({
                                            let key = key.clone();
                                            move || format!("ai-running-{key}")
                                        })
                                        .child(running_indicator(format!("ai-running-{key}"))),
                                )
                            })
                            .child(t!(phase.label_key())),
                    ),
            )
            .when(!target.is_empty(), |block| {
                block.child(
                    div()
                        .text_color(rgb(palette.text_muted))
                        .child(target.to_string()),
                )
            })
            .child(
                div()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("ai-command-body-{key}")
                    })
                    .min_w_0()
                    .w_full()
                    .rounded_sm()
                    .bg(rgb(palette.bg))
                    .px_2()
                    .py_1()
                    .font_family(gpui_code_font_family())
                    .child(self.highlighted_command(&card.command)),
            );
        if !card.explanation.trim().is_empty() {
            // This reasoning is already available in the owning message disclosure.
            let in_reasoning = snapshot.card_owner(&card.id).is_some_and(|id| {
                snapshot.messages.iter().any(|message| {
                    message.id == id
                        && is_agent_command_card(&card)
                        && message
                            .reasoning_content
                            .as_deref()
                            .is_some_and(|text| !text.trim().is_empty())
                })
            });
            if !in_reasoning {
                block = block.child(
                    div()
                        .text_color(rgb(palette.text_muted))
                        .child(card.explanation.clone()),
                );
            }
        }
        if phase == AiCommandPhase::NeedsApproval {
            block = block.child(div().text_color(rgb(palette.warning)).child(format!(
                "{}: {}",
                t!("ai.commandRisk"),
                localized_risk(card.risk_level.as_ref())
            )));
            if let Some(reason) = &card.risk_reason {
                block = block.child(div().text_color(rgb(palette.warning)).child(reason.clone()));
            }
        }
        if let Some(presentation) = linked {
            let step = &presentation.step;
            if !step.detail.is_empty()
                && matches!(
                    step.kind,
                    AiAgentStepKind::ToolProgress | AiAgentStepKind::Diagnostic
                )
            {
                block = block.child(div().text_color(rgb(color)).child(step.detail.clone()));
            }
            let thought_in_message = step.source_message_id.as_ref().is_some_and(|id| {
                snapshot.messages.iter().any(|message| {
                    &message.id == id
                        && (message.reasoning_content.is_some()
                            || extract_think_content(&message.content).1.is_some())
                })
            });
            if let Some(thought) = step.thought.as_ref().filter(|_| !thought_in_message) {
                let index = step.step_index;
                block = block.child(disclosure(
                    format!("ai-command-thought-{key}"),
                    t!("ai.thoughtProcess"),
                    presentation.thought_open,
                    cx.listener(move |panel, _, _, cx| {
                        panel.with_app(cx, move |app, cx| {
                            app.toggle_ai_agent_thought_expanded(index, cx)
                        })
                    }),
                ));
                if presentation.thought_open {
                    block = block.child(markdown_content_view(palette, thought));
                }
            }
            if let Some(output) = &step.observation {
                let index = step.step_index;
                block = block.child(disclosure(
                    format!("ai-command-output-{key}"),
                    t!("ai.executionOutput"),
                    presentation.output_open,
                    cx.listener(move |panel, _, _, cx| {
                        panel.with_app(cx, move |app, cx| {
                            app.toggle_ai_agent_output_expanded(index, cx)
                        })
                    }),
                ));
                if presentation.output_open {
                    block = block.child(
                        div()
                            .min_w_0()
                            .w_full()
                            .text_color(rgb(palette.text_muted))
                            .font_family(gpui_code_font_family())
                            .children(output.split('\n').map(|line| {
                                div()
                                    .min_w_0()
                                    .child(if line.is_empty() { " " } else { line }.to_string())
                            })),
                    );
                }
            }
        }
        let details_open = snapshot.expanded_command_details.contains(&key);
        if !card.expected_effect.is_empty()
            || card.rollback.is_some()
            || (phase != AiCommandPhase::NeedsApproval && card.risk_reason.is_some())
        {
            let id = key.clone();
            block = block.child(disclosure(
                format!("ai-command-details-{key}"),
                t!("ai.commandDetails"),
                details_open,
                cx.listener(move |panel, _, _, cx| {
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| {
                        app.ai.toggle_command_details(id);
                        app.defer_ai_panel_snapshot_flush(cx);
                    });
                }),
            ));
            if details_open {
                let mut details = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_color(rgb(palette.text_muted));
                if phase != AiCommandPhase::NeedsApproval {
                    details = details.child(format!(
                        "{}: {}",
                        t!("ai.commandRisk"),
                        localized_risk(card.risk_level.as_ref())
                    ));
                    if let Some(reason) = &card.risk_reason {
                        details = details.child(reason.clone());
                    }
                }
                if !card.expected_effect.is_empty() {
                    details = details.child(format!(
                        "{}: {}",
                        t!("ai.expectedEffect"),
                        card.expected_effect
                    ));
                }
                if let Some(rollback) = &card.rollback {
                    details = details.child(format!("{}: {rollback}", t!("ai.rollback")));
                }
                block = block.child(details);
            }
        }
        let mut actions = div().flex().flex_wrap().items_center().gap_1();
        let command = card.command.clone();
        actions = actions.child(secondary(
            format!("ai-command-copy-{key}"),
            t!("ai.copyCommand"),
            NyaButtonVariant::Secondary,
            cx.listener(move |_, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
            }),
        ));
        if phase.offers_approval() || phase.offers_run() {
            let id = key.clone();
            actions = actions.child(
                div()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("ai-command-run-{key}")
                    })
                    .child(
                        NyaButton::new(
                            format!("ai-command-run-{key}"),
                            if phase.offers_approval() {
                                t!("ai.approveCommand")
                            } else {
                                t!("ai.runCommand")
                            },
                        )
                        .small()
                        .variant(NyaButtonVariant::Primary)
                        .on_click(cx.listener(move |panel, _, _, cx| {
                            let id = id.clone();
                            panel
                                .with_app(cx, move |app, cx| app.run_ai_command_card_by_id(id, cx));
                        })),
                    ),
            );
        }
        if phase.offers_approval() {
            let id = key.clone();
            actions = actions.child(secondary(
                format!("ai-command-reject-{key}"),
                t!("ai.rejectCommand"),
                NyaButtonVariant::Danger,
                cx.listener(move |panel, _, _, cx| {
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| {
                        app.reject_ai_agent_command_card_by_id(id, cx)
                    });
                }),
            ));
        }
        if phase.offers_reuse() {
            let id = key.clone();
            actions = actions.child(secondary(
                format!("ai-command-insert-{key}"),
                t!("ai.insertCommand"),
                NyaButtonVariant::Primary,
                cx.listener(move |panel, _, _, cx| {
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| app.insert_ai_command_card_by_id(id, cx));
                }),
            ));
            let id = key.clone();
            actions = actions.child(secondary(
                format!("ai-command-save-{key}"),
                t!("ai.saveCommand"),
                NyaButtonVariant::Ghost,
                cx.listener(move |panel, _, _, cx| {
                    let id = id.clone();
                    panel.with_app(cx, move |app, cx| app.save_ai_command_card_by_id(id, cx));
                }),
            ));
        }
        block.child(actions).into_any_element()
    }
}
