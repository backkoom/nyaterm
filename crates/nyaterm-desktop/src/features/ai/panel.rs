use std::sync::Arc;

use rust_i18n::t;

use gpui::{
    ClipboardItem, Context, Entity, FontWeight, IntoElement, MouseButton, RenderImage, Rgba,
    ScrollHandle, SharedString, WeakEntity, Window, div, img, prelude::*, px, rgb, rgba, svg,
};
use nyaterm_core::{
    AgentCommandExecutionMode, AiAction, AiAgentKind, AiCommandCard, AiMessage, AiMode,
    AiModelConfigItem, AiProviderKind, AiReasoningEffort, AiSession, AiSessionScopeType,
    truncate_preview,
};
use nyaterm_ui::chat::{NyaMessageScroller, NyaMessageScrollerState};
use nyaterm_ui::{
    NyaDropdownMenu, NyaInputShell, NyaMenuAnchor, NyaMenuItem, NyaScrollable, NyaSearchInput,
};

use crate::features::NyaTermApp;
use crate::features::formatting::{group_ai_sessions_by_date, short_id};
use crate::features::text_inputs::TextInputSetup;
use crate::features::view_widgets::{full_window_input_layer, tab_menu_separator};
use crate::models::{
    AiDetectedErrorState, AiMessageMenuState, AiPreparedRequest, NavItem, SettingsTab,
};
use crate::theme::ThemePalette;
use crate::widgets::{small_button, svg_icon_button};

use super::presentation::AiResponsePhase;
use crate::features::runtime_jobs::AiAgentStepView;

mod command_syntax;
mod components;
mod content;
mod execution;
pub(super) mod harness;
mod index;
mod messages;
mod transcript;
use components::{ai_message_menu_button, ai_message_menu_position, ai_send_button, ai_setup_step};
use transcript::{AiTranscriptRow, AiTranscriptUpdate};

#[derive(Clone, Copy)]
pub(in crate::features) struct AiPanelChrome {
    pub palette: ThemePalette,
    pub transparent_surface: Rgba,
    pub transparent_section_header: Rgba,
    pub surface: Rgba,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

#[derive(Clone)]
pub(in crate::features) struct AiModelChoice {
    pub model: AiModelConfigItem,
    pub provider_label: String,
    pub provider_kind: Option<AiProviderKind>,
    pub provider_icon: Option<Arc<RenderImage>>,
}

#[derive(Clone)]
pub(in crate::features) struct AiMentionCandidate {
    pub session_id: String,
    pub label: String,
    pub kind: String,
    pub selected: bool,
}

#[derive(Clone)]
pub(in crate::features) struct AiTargetSession {
    pub session_id: String,
    pub label: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::features) struct AiAgentStepPresentation {
    pub step: AiAgentStepView,
    pub thought_open: bool,
    pub output_open: bool,
}

#[derive(Clone)]
pub(in crate::features) struct AiPanelSnapshot {
    index: Arc<index::AiSnapshotIndex>,
    pub native_run: Option<super::state::harness::NativeRunView>,
    pub native_answer_inputs: Vec<Entity<nyaterm_ui::NyaInputState>>,
    pub chrome: AiPanelChrome,
    pub ui_font_family: SharedString,
    pub enabled: bool,
    pub agent_mode: bool,
    pub running: bool,
    pub agent_kind: AiAgentKind,
    pub codex_enabled: bool,
    pub claude_code_enabled: bool,
    pub reasoning_effort: AiReasoningEffort,
    pub reasoning_choices: Arc<[AiReasoningEffort]>,
    pub external_agent: bool,
    pub external_model_label: String,
    pub selected_model_id: Option<String>,
    pub selected_model_exists: bool,
    pub model_label: String,
    pub selected_provider_kind: Option<AiProviderKind>,
    pub selected_provider_icon: Option<Arc<RenderImage>>,
    pub enabled_models: Arc<[AiModelConfigItem]>,
    pub model_choices: Arc<[AiModelChoice]>,
    pub discovery_menu_open: bool,
    pub discovery_index: usize,
    pub prompt_draft: String,
    pub prompt_input: Entity<nyaterm_ui::NyaInputState>,
    pub model_search_input: Option<Entity<nyaterm_ui::NyaInputState>>,
    pub history_search_input: Option<Entity<nyaterm_ui::NyaInputState>>,
    pub file_action_ready: bool,
    pub messages: Arc<[Arc<AiMessage>]>,
    pub streaming_assistant_id: Option<String>,
    pub response_phase: AiResponsePhase,
    pub expanded_message_thoughts: Arc<[String]>,
    pub expanded_command_details: Arc<[String]>,
    pub expanded_command_scripts: Arc<[String]>,
    pub agent_history_expanded: bool,
    pub expanded_execution_groups: Arc<[String]>,
    pub command_cards: Arc<[AiCommandCard]>,
    pub agent_steps: Arc<[AiAgentStepPresentation]>,
    pub target_sessions: Arc<[AiTargetSession]>,
    pub mention_open: bool,
    pub mention_index: usize,
    pub mention_candidates: Arc<[AiMentionCandidate]>,
    pub quoted_text: Option<String>,
    pub detected_error: Option<AiDetectedErrorState>,
    pub message_menu: Option<AiMessageMenuState>,
    pub history_open: bool,
    pub history_query: String,
    pub history_sessions: Arc<[AiSession]>,
    pub history_running_ids: Arc<[String]>,
    pub current_ai_session_id: String,
    pub owner_terminal_id: Option<String>,
    pub owner_connection_id: Option<String>,
    pub history_pending: bool,
    pub history_error: Option<String>,
    pub history_actions_disabled: bool,
    pub execution_menu_open: bool,
    pub command_execution_mode: AgentCommandExecutionMode,
    pub background_execution_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::features) struct AiHeaderPresentation {
    pub running: bool,
    pub selected_model_id: Option<String>,
    pub model_label: String,
    pub execution_mode: AgentCommandExecutionMode,
}

pub(in crate::features) struct AiPanel {
    app: WeakEntity<NyaTermApp>,
    snapshot: Option<AiPanelSnapshot>,
    transcript_scroll: Entity<NyaMessageScrollerState>,
    transcript_rows: Arc<[AiTranscriptRow]>,
    transcript_text_style: Option<(gpui::TextStyle, gpui::Pixels, String)>,
    command_syntax: command_syntax::CommandSyntaxCache,
    content: content::ContentCache,
    mention_scroll: ScrollHandle,
    model_scroll: ScrollHandle,
    history_scroll: ScrollHandle,
    picker_reveal_pending: bool,
    focused_question: Option<(String, String)>,
    #[cfg(test)]
    paint_count: usize,
    #[cfg(test)]
    snapshot_set_count: usize,
}

impl AiPanel {
    pub(in crate::features) fn new(app: WeakEntity<NyaTermApp>, cx: &mut Context<Self>) -> Self {
        let transcript_scroll = cx.new(|cx| NyaMessageScrollerState::new(0, cx));
        cx.observe(&transcript_scroll, |_, _, cx| cx.notify())
            .detach();
        Self {
            app,
            snapshot: None,
            transcript_scroll,
            transcript_rows: Arc::from([]),
            transcript_text_style: None,
            command_syntax: command_syntax::CommandSyntaxCache::default(),
            content: content::ContentCache::default(),
            mention_scroll: ScrollHandle::new(),
            model_scroll: ScrollHandle::new(),
            history_scroll: ScrollHandle::new(),
            picker_reveal_pending: false,
            focused_question: None,
            #[cfg(test)]
            paint_count: 0,
            #[cfg(test)]
            snapshot_set_count: 0,
        }
    }

    pub(in crate::features) fn set_snapshot(
        &mut self,
        mut snapshot: AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) {
        snapshot.index = Arc::new(index::AiSnapshotIndex::build(&snapshot));
        self.refresh_content(&snapshot, cx);
        self.refresh_command_syntax(&snapshot, cx);
        let rows = AiTranscriptRow::project(&snapshot);
        let update = AiTranscriptUpdate::between(
            self.snapshot.as_ref(),
            &snapshot,
            &self.transcript_rows,
            &rows,
        );
        self.transcript_scroll.update(cx, |state, cx| {
            if update.reset {
                state.reset(rows.len(), cx);
            } else {
                if let Some((range, count)) = update.splice {
                    state.splice(range, count, cx);
                }
                if update.remeasure_all {
                    state.remeasure(cx);
                } else {
                    for range in update.remeasure {
                        state.remeasure_items(range, cx);
                    }
                }
            }
        });
        self.transcript_rows = rows.into();
        if snapshot.mention_open
            && self.snapshot.as_ref().is_none_or(|previous| {
                !previous.mention_open
                    || previous.mention_index != snapshot.mention_index
                    || previous.prompt_draft != snapshot.prompt_draft
            })
        {
            self.mention_scroll.scroll_to_item(snapshot.mention_index);
            self.picker_reveal_pending = true;
        }
        if snapshot.discovery_menu_open
            && self.snapshot.as_ref().is_none_or(|previous| {
                !previous.discovery_menu_open
                    || previous.discovery_index != snapshot.discovery_index
                    || previous.reasoning_choices != snapshot.reasoning_choices
                    || !previous
                        .model_choices
                        .iter()
                        .map(|choice| &choice.model.id)
                        .eq(snapshot.model_choices.iter().map(|choice| &choice.model.id))
            })
        {
            let index = snapshot.discovery_index
                + if snapshot.discovery_index < snapshot.reasoning_choices.len() {
                    1
                } else {
                    2
                };
            self.model_scroll.scroll_to_item(index);
            self.picker_reveal_pending = true;
        }
        self.snapshot = Some(snapshot);
        #[cfg(test)]
        {
            self.snapshot_set_count += 1;
        }
        cx.notify();
    }

    #[cfg(test)]
    pub(in crate::features) fn snapshot(&self) -> Option<&AiPanelSnapshot> {
        self.snapshot.as_ref()
    }

    #[cfg(test)]
    pub(in crate::features) fn paint_count(&self) -> usize {
        self.paint_count
    }

    #[cfg(test)]
    pub(in crate::features) fn snapshot_set_count(&self) -> usize {
        self.snapshot_set_count
    }

    pub(in crate::features) fn with_app<R: Default>(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut NyaTermApp, &mut Context<NyaTermApp>) -> R,
    ) -> R {
        let Some(app) = self.app.upgrade() else {
            return R::default();
        };
        app.update(cx, |app, cx| {
            let before = app.ai_header_presentation();
            let result = f(app, cx);
            app.defer_ai_panel_snapshot_flush(cx);
            app.notify_root_if_ai_header_changed(before, cx);
            result
        })
    }

    fn panel(&self) -> Option<&AiPanelSnapshot> {
        self.snapshot.as_ref()
    }

    fn palette(&self) -> ThemePalette {
        self.panel()
            .map(|snapshot| snapshot.chrome.palette)
            .expect("AI panel render requires a snapshot")
    }

    fn render_panel(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(snapshot) = self.snapshot.clone() else {
            return div().size_full().into_any_element();
        };
        let palette = snapshot.chrome.palette;
        let prompt_input = NyaInputShell::new("ai.chat.prompt", &snapshot.prompt_input)
            .multi_line()
            .gpui_context_menu([
                t!("menu.cut").into(),
                t!("menu.copy").into(),
                t!("menu.paste").into(),
                t!("menu.selectAll").into(),
            ])
            .height(px(64.))
            .into_any_element();
        let model_search_input = snapshot
            .model_search_input
            .as_ref()
            .map(|field| NyaSearchInput::new("ai-model-search", field).into_any_element());
        let panel_entity = cx.weak_entity();
        let composer_disabled = snapshot.running || snapshot.history_pending || !snapshot.enabled;
        let send_disabled = !snapshot.running
            && (snapshot.history_pending
                || !snapshot.enabled
                || (!snapshot.external_agent && !snapshot.selected_model_exists)
                || snapshot.prompt_draft.trim().is_empty());

        div()
            .tab_group()
            .font_family(snapshot.ui_font_family.clone())
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(snapshot.chrome.transparent_surface)
            .relative()
            .on_children_prepainted(move |_, _, cx| {
                let Some(panel) = panel_entity.upgrade() else {
                    return;
                };
                if !panel.read(cx).picker_reveal_pending {
                    return;
                }
                // A mounted handle learns its viewport and overflow mode during
                // prepaint. Queue reveal only after those bounds are measured,
                // including when filtering moves a bottom-anchored popup.
                cx.defer(move |cx| {
                    panel.update(cx, |panel, cx| {
                        if !std::mem::take(&mut panel.picker_reveal_pending) {
                            return;
                        }
                        if let Some(snapshot) = panel.snapshot.as_ref() {
                            if snapshot.mention_open {
                                panel.mention_scroll.scroll_to_item(snapshot.mention_index);
                            }
                            if snapshot.discovery_menu_open {
                                let headings = if snapshot.discovery_index
                                    < snapshot.reasoning_choices.len()
                                {
                                    1
                                } else {
                                    2
                                };
                                panel
                                    .model_scroll
                                    .scroll_to_item(snapshot.discovery_index + headings);
                            }
                        }
                        cx.notify();
                    })
                });
            })
            .when_some(snapshot.detected_error.clone(), |this, detected| {
                this.child(self.ai_detected_error_banner(&snapshot, detected, cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .relative()
                    .child(
                        div()
                            .id(SharedString::from("ai-transcript-scroll"))
                            .debug_selector(|| "ai-transcript-viewport".to_string())
                            .size_full()
                            .flex()
                            .flex_col()
                            .child(self.ai_transcript_body(&snapshot, cx)),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "ai-composer".to_string())
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(palette.border))
                    .bg(snapshot.chrome.transparent_section_header)
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when_some(snapshot.quoted_text.clone(), |this, quoted_text| {
                        this.child(self.ai_quote_bar(palette, quoted_text, cx))
                    })
                    .when(!snapshot.target_sessions.is_empty(), |this| {
                        this.child(self.ai_target_sessions_row(&snapshot, cx))
                    })
                    .child(
                        div()
                            .debug_selector(|| "ai-prompt".to_string())
                            .w_full()
                            .relative()
                            .flex()
                            .when(composer_disabled, |this| this.opacity(0.56))
                            .on_key_down(cx.listener(|panel, event: &gpui::KeyDownEvent, _, cx| {
                                if panel.with_app(cx, |app, cx| {
                                    app.handle_ai_prompt_key_down(event, cx)
                                }) {
                                    cx.stop_propagation();
                                }
                            }))
                            .child(div().min_w_0().flex_1().child(prompt_input))
                            .when(snapshot.mention_open, |this| {
                                this.child(self.ai_mention_popover(&snapshot, cx))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(self.ai_mode_switch(&snapshot, cx))
                                    .when(snapshot.external_agent, |this| {
                                        this.child(ai_agent_status_badge(&snapshot))
                                    })
                                    .when(!snapshot.external_agent, |this| {
                                        this.child(self.ai_model_selector(
                                            &snapshot,
                                            model_search_input,
                                            cx,
                                        ))
                                    }),
                            )
                            .child(ai_send_button(palette, snapshot.running, send_disabled, cx)),
                    )
                    .when(snapshot.file_action_ready, |this| {
                        this.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(palette.warning))
                                .child(t!("ai.fileActionReady")),
                        )
                    }),
            )
            // Absolute popovers must paint after the transcript and composer;
            // later siblings otherwise cover an open menu even though its
            // position is correct.
            .when(snapshot.history_open, |this| {
                this.child(self.ai_history_popover(&snapshot, cx))
            })
            .when(snapshot.execution_menu_open, |this| {
                this.child(self.ai_execution_mode_menu(&snapshot, cx))
            })
            .when_some(snapshot.message_menu.clone(), |this, menu| {
                this.child(self.ai_message_context_menu_overlay(&snapshot, menu, cx))
            })
            .into_any_element()
    }

    fn ai_detected_error_banner(
        &self,
        snapshot: &AiPanelSnapshot,
        detected: AiDetectedErrorState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let analyze_state = detected.clone();
        div()
            .flex_none()
            .border_b_1()
            .border_color(rgb(palette.border))
            .bg(rgba(0xf59e0b1a))
            .px_3()
            .py_2()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(0xd97706))
                            .child(t!("ai.errorDetected")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(format!("session {}", short_id(&detected.session_id))),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(small_button(
                        palette,
                        "ai-detected-error-analyze",
                        t!("ai.analyze"),
                        cx.listener(move |panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.analyze_ai_detected_error(analyze_state.clone(), cx);
                            });
                        }),
                    ))
                    .child(small_button(
                        palette,
                        "ai-detected-error-close",
                        t!("common.close"),
                        cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.dismiss_ai_detected_error(cx);
                            });
                        }),
                    )),
            )
    }

    fn ai_quote_bar(
        &self,
        palette: ThemePalette,
        quoted_text: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.link))
            .bg(rgb(palette.hover))
            .flex()
            .items_center()
            .gap_2()
            .overflow_hidden()
            .child(div().w(px(3.)).h(px(28.)).flex_none().bg(rgb(palette.link)))
            .child(
                div()
                    .flex_none()
                    .text_size(px(11.))
                    .text_color(rgb(palette.link))
                    .child(t!("ai.quote")),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .py_1()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .overflow_hidden()
                    .child(truncate_preview(quoted_text.trim(), 140)),
            )
            .child(
                div()
                    .id(SharedString::from("ai-quote-clear"))
                    .size(px(20.))
                    .mr_1()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .text_color(rgb(palette.text_muted))
                    .cursor_pointer()
                    .hover(move |this| {
                        this.bg(rgb(palette.surface_elevated))
                            .text_color(rgb(palette.text))
                    })
                    .on_click(cx.listener(|panel, _, _, cx| {
                        panel.with_app(cx, |app, cx| {
                            app.clear_ai_quote(cx);
                        });
                    }))
                    .child(
                        svg()
                            .size(px(13.))
                            .path("icons/window/close.svg")
                            .text_color(rgb(palette.text_muted)),
                    ),
            )
    }

    fn ai_target_sessions_row(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let mut target_row = div().flex().flex_wrap().items_center().gap_1().child(
            div()
                .text_size(px(10.))
                .font_weight(FontWeight(600.))
                .text_color(rgb(palette.text_muted))
                .child(format!("{}:", t!("ai.targetSession"))),
        );
        for target in snapshot.target_sessions.iter() {
            let session_id = target.session_id.clone();
            let label = target.label.clone();
            target_row = target_row.child(
                div()
                    .min_w_0()
                    .max_w(px(220.))
                    .h(px(20.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(palette.link))
                    .bg(rgb(palette.hover))
                    .text_size(px(10.))
                    .font_weight(FontWeight(600.))
                    .text_color(rgb(palette.link))
                    .child(
                        div()
                            .size(px(6.))
                            .rounded_full()
                            .flex_none()
                            .bg(rgb(palette.link)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .child(truncate_preview(&label, 32)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("ai-target-remove-{session_id}")))
                            .size(px(14.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .cursor_pointer()
                            .hover(move |this| {
                                this.bg(rgb(palette.surface_elevated))
                                    .text_color(rgb(palette.danger))
                            })
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                let session_id = session_id.clone();
                                panel.with_app(cx, move |app, cx| {
                                    app.remove_ai_target_session(session_id, cx);
                                });
                            }))
                            .child(
                                svg()
                                    .size(px(11.))
                                    .path("icons/window/close.svg")
                                    .text_color(rgb(palette.text_muted)),
                            ),
                    ),
            );
        }
        target_row
    }

    fn ai_mention_popover(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let popover = div()
            .absolute()
            .bottom(gpui::relative(1.))
            .mb_1()
            .left_0()
            .right_0()
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(snapshot.chrome.surface)
            .shadow_lg()
            .flex()
            .flex_col()
            .p_1();
        if snapshot.mention_candidates.is_empty() {
            return popover
                .child(
                    div()
                        .h(px(44.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(palette.text_muted))
                        .child(t!("ai.noSessions")),
                )
                .into_any_element();
        }
        let mut rows = div()
            .id("ai-mention-list")
            .max_h(px(192.))
            .overflow_y_scroll()
            .track_scroll(&self.mention_scroll)
            .flex()
            .flex_col();
        for (index, candidate) in snapshot.mention_candidates.iter().enumerate() {
            let focused = index == snapshot.mention_index;
            let candidate = candidate.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!(
                        "ai-mention-session-{}",
                        candidate.session_id
                    )))
                    .debug_selector(move || format!("ai-mention-row-{index}"))
                    .h(px(30.))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_sm()
                    .bg(if focused || candidate.selected {
                        rgb(palette.hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(rgb(palette.hover)))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.with_app(cx, move |app, cx| {
                            app.ai.set_chat_mention_index(index);
                            app.select_ai_mention_candidate(cx);
                        });
                    }))
                    .child(div().size(px(7.)).rounded_full().flex_none().bg(
                        if candidate.selected {
                            rgb(palette.link)
                        } else {
                            rgb(palette.text_dimmed)
                        },
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(px(11.))
                            .font_weight(FontWeight(600.))
                            .text_color(rgb(palette.text))
                            .child(truncate_preview(&candidate.label, 34)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(candidate.kind),
                    ),
            );
        }
        popover
            .child(
                div()
                    .relative()
                    .child(rows)
                    .vertical_scrollbar(&self.mention_scroll),
            )
            .into_any_element()
    }

    fn ai_mode_switch(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let modes = [
            (AiMode::Ask, AiAgentKind::Nyaterm, t!("ai.modeAsk"), true),
            (
                AiMode::Agent,
                AiAgentKind::Nyaterm,
                t!("ai.modeNyatermAgent"),
                true,
            ),
            (
                AiMode::Agent,
                AiAgentKind::Codex,
                t!("ai.modeCodexAgent"),
                snapshot.codex_enabled,
            ),
            (
                AiMode::Agent,
                AiAgentKind::ClaudeCode,
                t!("ai.modeClaudeCodeAgent"),
                snapshot.claude_code_enabled,
            ),
        ];
        let mut items = Vec::new();
        let mut selected_label = t!("ai.modeAsk");
        for (mode, kind, label, enabled) in modes {
            let selected = if mode == AiMode::Ask {
                !snapshot.agent_mode
            } else {
                snapshot.agent_mode && snapshot.agent_kind == kind
            };
            if selected {
                selected_label = label.clone();
            }
            items.push(
                NyaMenuItem::action(label)
                    .checked(selected)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        panel.with_app(cx, |app, cx| {
                            app.set_ai_run_mode(mode.clone(), kind.clone(), cx);
                            app.focus_text_input_if_present("ai.chat.prompt", window, cx);
                        });
                    })),
            );
        }
        div()
            .debug_selector(|| "ai-mode-control".to_string())
            .w(px(108.))
            .min_w_0()
            .max_w(gpui::relative(0.45))
            .flex_none()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.input))
            .child(
                NyaDropdownMenu::new("ai-mode-selector")
                    .anchor(NyaMenuAnchor::BottomLeft)
                    .min_width(px(170.))
                    .content(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_size(px(11.))
                                    .text_ellipsis()
                                    .child(selected_label),
                            )
                            .child(
                                svg()
                                    .size(px(12.))
                                    .flex_none()
                                    .path("icons/chevron-down.svg"),
                            ),
                    )
                    .items(items),
            )
    }

    fn ai_model_selector(
        &self,
        snapshot: &AiPanelSnapshot,
        model_search_input: Option<gpui::AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let content = div()
            .min_w_0()
            .flex_1()
            .flex()
            .items_center()
            .gap_1()
            .when(snapshot.selected_model_exists, |this| {
                this.child(ai_model_provider_badge(
                    palette,
                    snapshot.selected_provider_kind.as_ref(),
                    snapshot.selected_provider_icon.as_ref(),
                ))
            })
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_size(px(11.))
                    .text_ellipsis()
                    .child(snapshot.model_label.clone()),
            )
            .when(snapshot.selected_model_exists, |this| {
                this.child(
                    div()
                        .flex_none()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .child(format!(
                            "· {}",
                            super::reasoning_effort_label(&snapshot.reasoning_effort)
                        )),
                )
            })
            .child(
                svg()
                    .size(px(12.))
                    .flex_none()
                    .path("icons/chevron-down.svg"),
            );
        div()
            .min_w_0()
            .flex_1()
            .relative()
            .child(
                div()
                    .debug_selector(|| "ai-model-control".to_string())
                    .h(px(28.))
                    .min_w_0()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(rgb(palette.input))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        nyaterm_ui::NyaButton::new("ai-model-selector", "")
                            .variant(nyaterm_ui::NyaButtonVariant::Ghost)
                            .small()
                            .height(px(28.))
                            .full_width()
                            .content(content)
                            .disabled(snapshot.enabled_models.is_empty())
                            .tooltip(format!(
                                "{} · {}",
                                snapshot.model_label,
                                super::reasoning_effort_label(&snapshot.reasoning_effort)
                            ))
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.with_app(cx, |app, cx| {
                                    let selected_index = app.ai_selected_model_index();
                                    if app.ai.toggle_discovery_menu(selected_index) {
                                        app.reset_text_input("ai.model-search", "", cx);
                                        let field = app.text_input(
                                            "ai.model-search",
                                            "",
                                            TextInputSetup::placeholder(t!("ai.searchModels")),
                                            cx,
                                        );
                                        window.focus(&field.read(cx).focus_handle(), cx);
                                    } else {
                                        app.focus_text_input_if_present(
                                            "ai.chat.prompt",
                                            window,
                                            cx,
                                        );
                                    }
                                });
                            })),
                    ),
            )
            .when(snapshot.discovery_menu_open, |this| {
                this.child(self.ai_model_menu(
                    snapshot,
                    snapshot.selected_model_id.clone(),
                    model_search_input,
                    cx,
                ))
            })
    }

    fn ai_model_menu(
        &self,
        snapshot: &AiPanelSnapshot,
        selected_id: Option<String>,
        model_search_input: Option<gpui::AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let mut menu = div()
            .absolute()
            // The selector shares its row with the mode switch and send
            // button. Expanding from its left edge made the fixed-width menu
            // cross the side-panel boundary; anchoring the trailing edges
            // keeps the popup inside the panel at its normal width.
            .right_0()
            .bottom(px(34.))
            .w(px(260.))
            .max_h(px(360.))
            .overflow_hidden()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(snapshot.chrome.surface)
            .shadow_lg()
            .p_1()
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        if snapshot.enabled_models.is_empty() {
            return menu
                .child(
                    div()
                        .px_2()
                        .py_2()
                        .text_size(px(11.))
                        .text_color(rgb(palette.text_muted))
                        .child(t!("ai.noEnabledModels")),
                )
                .child(
                    div()
                        .id(SharedString::from("ai-model-open-settings"))
                        .h(px(28.))
                        .px_2()
                        .flex()
                        .items_center()
                        .rounded_sm()
                        .cursor_pointer()
                        .text_size(px(11.))
                        .text_color(rgb(palette.link))
                        .hover(move |this| this.bg(rgb(palette.hover)))
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.ai.close_discovery_menu();
                                app.shell.set_settings_active_tab(SettingsTab::AiModels);
                                app.open_page(NavItem::Settings, cx);
                            });
                        }))
                        .child(t!("ai.models")),
                );
        }
        if let Some(model_search_input) = model_search_input {
            menu = menu.child(
                div()
                    .mb_1()
                    .on_key_down(
                        cx.listener(|panel, event: &gpui::KeyDownEvent, window, cx| {
                            if panel.with_app(cx, |app, cx| {
                                let handled = app.handle_ai_model_search_key_down(event, cx);
                                if handled && !app.ai.discovery_menu_is_open() {
                                    app.focus_text_input_if_present("ai.chat.prompt", window, cx);
                                }
                                handled
                            }) {
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .child(model_search_input),
            );
        }
        let mut rows = div()
            .id(SharedString::from("ai-model-choice-list"))
            .min_h_0()
            .max_h(px(300.))
            .overflow_y_scroll()
            .track_scroll(&self.model_scroll)
            .flex()
            .flex_col()
            .child(ai_menu_heading(palette, t!("ai.reasoningIntensity")));
        for (index, effort) in snapshot.reasoning_choices.iter().enumerate() {
            let effort = effort.clone();
            let selected = snapshot.reasoning_effort == effort;
            let label = super::reasoning_effort_label(&effort);
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("ai-reasoning-choice-{index}")))
                    .h(px(28.))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_sm()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text))
                    .bg(if snapshot.discovery_index == index {
                        rgb(palette.hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(rgb(palette.hover)))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel.with_app(cx, |app, cx| {
                            app.ai.set_discovery_index(index);
                            app.set_ai_reasoning_effort(effort.clone(), cx);
                        });
                    }))
                    .child(ai_choice_check(palette, selected))
                    .child(label),
            );
        }
        rows = rows.child(ai_menu_heading(palette, t!("ai.models")));
        if snapshot.model_choices.is_empty() {
            rows = rows.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("ai.noModelMatches")),
            );
        }
        for (index, choice) in snapshot.model_choices.iter().enumerate() {
            let model = choice.model.clone();
            let provider_label = choice.provider_label.clone();
            let model_id = model.id.clone();
            let is_selected = selected_id.as_deref() == Some(model.id.as_str());
            let choice_index = index + snapshot.reasoning_choices.len();
            let focused = choice_index == snapshot.discovery_index;
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("ai-model-choice-{}", model.id)))
                    .h(px(34.))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_sm()
                    .bg(if focused || is_selected {
                        rgb(palette.hover)
                    } else {
                        rgba(0x00000000)
                    })
                    .cursor_pointer()
                    .hover(move |this| this.bg(rgb(palette.hover)))
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        let model_id = model_id.clone();
                        panel.with_app(cx, move |app, cx| {
                            app.ai.set_discovery_index(choice_index);
                            app.ai.close_discovery_menu();
                            app.set_ai_default_model(model_id, cx);
                            app.focus_text_input_if_present("ai.chat.prompt", window, cx);
                        });
                    }))
                    .child(
                        div()
                            .size(px(14.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(palette.link))
                            .when(is_selected, |this| {
                                this.child(
                                    svg()
                                        .size(px(13.))
                                        .path("icons/check.svg")
                                        .text_color(rgb(palette.link)),
                                )
                            }),
                    )
                    .child(ai_model_provider_badge(
                        palette,
                        choice.provider_kind.as_ref(),
                        choice.provider_icon.as_ref(),
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_size(px(11.))
                            .font_weight(FontWeight(600.))
                            .text_color(rgb(palette.text))
                            .child(truncate_preview(&model.name, 32)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(120.))
                            .overflow_hidden()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(truncate_preview(&provider_label, 24)),
                    ),
            );
        }
        menu.child(
            div()
                .relative()
                .min_h_0()
                .child(rows)
                .vertical_scrollbar(&self.model_scroll),
        )
    }

    fn ai_transcript_body(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        if self.transcript_rows.is_empty() {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .px_3()
                .py_2()
                .child(self.ai_empty_transcript(snapshot, cx))
                .into_any_element();
        }
        let panel = cx.weak_entity();
        let snapshot = snapshot.clone();
        let rows = Arc::clone(&self.transcript_rows);
        let mut row_style = gpui::StyleRefinement::default();
        row_style.padding.bottom = Some(px(8.).into());
        NyaMessageScroller::new(
            "ai-transcript",
            self.transcript_scroll.clone(),
            move |index, _, cx| {
                panel
                    .update(cx, |panel, cx| {
                        panel.ai_transcript_row(&snapshot, &rows[index], cx)
                    })
                    .unwrap_or_else(|_| div().into_any_element())
            },
        )
        .size_full()
        .with_row_style(row_style)
        .with_jump_button_label(t!("ai.jumpToLatest"))
        .into_any_element()
    }

    fn ai_empty_transcript(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = snapshot.chrome.palette;
        let has_model = snapshot.external_agent
            || snapshot.selected_model_id.is_some()
            || !snapshot.enabled_models.is_empty();
        if !snapshot.enabled {
            return div()
                .flex_1()
                .min_h(px(192.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_3()
                .child(
                    svg()
                        .size(px(36.))
                        .path("icons/ai.svg")
                        .text_color(rgb(palette.text_muted)),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(palette.text_muted))
                        .child(t!("ai.goToSettingsToEnable")),
                )
                .into_any_element();
        }
        if !has_model {
            return div()
                .flex_1()
                .min_h(px(240.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_3()
                .px_4()
                .child(
                    div()
                        .size(px(48.))
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(0x9e6a03))
                        .bg(rgb(0x3d2e00))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::features::view_widgets::mono_icon(
                            "icons/warning.svg",
                            rgb(palette.warning).into(),
                            22.,
                        )),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text))
                        .child(t!("ai.setupTitle")),
                )
                .child(ai_setup_step(palette, "1", t!("ai.setupStep1")))
                .child(ai_setup_step(palette, "2", t!("ai.setupStep2")))
                .child(
                    div()
                        .id(SharedString::from("ai-empty-open-settings-setup"))
                        .mt_1()
                        .h(px(30.))
                        .px_3()
                        .rounded_md()
                        .bg(rgb(palette.success))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(12.))
                        .font_weight(FontWeight(600.))
                        .text_color(rgb(0xffffff))
                        .cursor_pointer()
                        .hover(|this| this.bg(rgb(0x2ea043)))
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.shell.set_settings_active_tab(SettingsTab::AiGeneral);
                                app.open_page(NavItem::Settings, cx);
                            });
                        }))
                        .child(t!("ai.setupAction")),
                )
                .into_any_element();
        }
        div()
            .debug_selector(|| "ai-empty-transcript".to_string())
            .flex_1()
            .min_h(px(180.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_3()
            .child(
                svg()
                    .size(px(40.))
                    .path("icons/ai.svg")
                    .text_color(rgb(palette.text_muted)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("ai.empty")),
            )
            .into_any_element()
    }

    fn ai_execution_mode_menu(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        div()
            .id(SharedString::from("ai-execution-mode-menu"))
            .absolute()
            .top(px(4.))
            .right(px(8.))
            .w(px(260.))
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(snapshot.chrome.surface)
            .shadow_lg()
            .py_1()
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(11.))
                    .font_weight(FontWeight(700.))
                    .text_color(rgb(palette.text))
                    .child(t!("ai.agentCommandExecutionMode")),
            )
            .child(self.ai_execution_mode_item(
                "ai-exec-confirm",
                t!("ai.executionModeConfirmEach"),
                t!("ai.executionModeConfirmEachDesc"),
                AgentCommandExecutionMode::ConfirmEach,
                snapshot.command_execution_mode == AgentCommandExecutionMode::ConfirmEach,
                cx,
            ))
            .child(self.ai_execution_mode_item(
                "ai-exec-smart",
                t!("ai.executionModeSmart"),
                t!("ai.executionModeSmartDesc"),
                AgentCommandExecutionMode::Smart,
                snapshot.command_execution_mode == AgentCommandExecutionMode::Smart,
                cx,
            ))
            .child(self.ai_execution_mode_item(
                "ai-exec-auto",
                t!("ai.executionModeAuto"),
                t!("ai.executionModeAutoDesc"),
                AgentCommandExecutionMode::Auto,
                snapshot.command_execution_mode == AgentCommandExecutionMode::Auto,
                cx,
            ))
            .child(tab_menu_separator(palette))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(11.))
                    .font_weight(FontWeight(700.))
                    .text_color(rgb(palette.text))
                    .child(t!("ai.executionMethod")),
            )
            .child(self.ai_background_execution_item(snapshot, cx))
    }

    fn ai_background_execution_item(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let enabled = snapshot.background_execution_enabled;
        div()
            .id(SharedString::from("ai-exec-background"))
            .px_3()
            .py_2()
            .flex()
            .items_start()
            .gap_2()
            .cursor_pointer()
            .hover(move |this| this.bg(rgb(palette.surface_elevated)))
            .on_click(cx.listener(|panel, _, _, cx| {
                panel.with_app(cx, |app, cx| {
                    app.toggle_ai_background_execution(cx);
                });
            }))
            .child(
                div()
                    .mt(px(1.))
                    .size(px(14.))
                    .rounded_sm()
                    .border_1()
                    .border_color(if enabled {
                        rgb(palette.link)
                    } else {
                        rgb(palette.border)
                    })
                    .bg(if enabled {
                        rgb(palette.link)
                    } else {
                        rgb(palette.input)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(palette.bg))
                    .when(enabled, |this| {
                        this.child(
                            svg()
                                .size(px(11.))
                                .path("icons/check.svg")
                                .text_color(rgb(palette.bg)),
                        )
                    }),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight(600.))
                            .text_color(rgb(palette.text))
                            .child(t!("ai.backgroundAgentExecution")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(t!("ai.backgroundAgentExecutionDesc")),
                    ),
            )
    }

    fn ai_execution_mode_item(
        &self,
        id: &'static str,
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        mode: AgentCommandExecutionMode,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title: SharedString = title.into();
        let detail: SharedString = detail.into();
        let palette = self.palette();
        div()
            .id(SharedString::from(id))
            .px_3()
            .py_2()
            .flex()
            .items_start()
            .gap_2()
            .cursor_pointer()
            .hover(move |this| this.bg(rgb(palette.surface_elevated)))
            .on_click(cx.listener(move |panel, _, window, cx| {
                let mode = mode.clone();
                panel.with_app(cx, move |app, cx| {
                    if mode == AgentCommandExecutionMode::Auto
                        && app.ai.settings_config().agent_command_execution_mode
                            != AgentCommandExecutionMode::Auto
                    {
                        app.open_ai_auto_execution_confirm(window, cx);
                        return;
                    }
                    app.set_ai_command_mode(mode.clone(), cx);
                    app.ai.close_execution_menu();
                    app.ai.set_panel_status(format!(
                        "Agent execution mode: {}",
                        match mode {
                            AgentCommandExecutionMode::ConfirmEach => "confirm each",
                            AgentCommandExecutionMode::Smart => "smart",
                            AgentCommandExecutionMode::Auto => "auto",
                        }
                    ));
                });
            }))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight(600.))
                            .text_color(if selected {
                                rgb(palette.link)
                            } else {
                                rgb(palette.text)
                            })
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(palette.text_muted))
                            .child(detail),
                    ),
            )
            .child(
                div()
                    .size(px(14.))
                    .flex_none()
                    .text_color(rgb(palette.link))
                    .when(selected, |this| {
                        this.child(
                            svg()
                                .size(px(13.))
                                .path("icons/check.svg")
                                .text_color(rgb(palette.link)),
                        )
                    }),
            )
    }

    fn ai_history_popover(
        &self,
        snapshot: &AiPanelSnapshot,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let query = snapshot.history_query.trim().to_lowercase();
        let filtered: Vec<_> = snapshot
            .history_sessions
            .iter()
            .filter(|session| {
                query.is_empty()
                    || session.title.to_lowercase().contains(&query)
                    || session.id.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let total_count = snapshot.history_sessions.len();
        let filtered_count = filtered.len();
        let mut grouped = [
            (t!("ai.historyCurrentTerminal"), Vec::new()),
            (t!("ai.historySameConnection"), Vec::new()),
            (t!("ai.historyOtherSessions"), Vec::new()),
        ];
        for session in filtered {
            let group = if session.scope.r#type == AiSessionScopeType::Terminal
                && session.scope.target_id == snapshot.owner_terminal_id
                && snapshot.owner_terminal_id.is_some()
            {
                0
            } else if snapshot
                .owner_connection_id
                .as_ref()
                .is_some_and(|connection_id| {
                    session.connection_id.as_ref() == Some(connection_id)
                        || session.scope.connection_ids.contains(connection_id)
                })
            {
                1
            } else {
                2
            };
            grouped[group].1.push(session);
        }
        let mut search_input = snapshot.history_search_input.as_ref().map(|field| {
            NyaSearchInput::new("ai-history-search", field).on_key_down(cx.listener(
                |panel, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        cx.stop_propagation();
                        panel.with_app(cx, |app, cx| {
                            app.close_ai_history(window, cx);
                        });
                    }
                },
            ))
        });
        if !snapshot.history_query.is_empty()
            && let Some(input) = search_input.take()
        {
            search_input = Some(
                input.trailing(
                    div()
                        .id(SharedString::from("ai-history-search-clear"))
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .text_size(px(10.))
                        .text_color(rgb(palette.text_muted))
                        .cursor_pointer()
                        .hover(move |this| {
                            this.bg(rgb(palette.surface_elevated))
                                .text_color(rgb(palette.text))
                        })
                        .on_click(cx.listener(|panel, _, _, cx| {
                            panel.with_app(cx, |app, cx| {
                                app.ai.clear_history_query();
                                app.reset_text_input("ai.history-search", "", cx);
                            });
                        }))
                        .child(
                            svg()
                                .size(px(13.))
                                .path("icons/window/close.svg")
                                .text_color(rgb(palette.text_muted)),
                        ),
                ),
            );
        }

        let mut rows = div().flex().flex_col().gap_1().p_2();
        if filtered_count == 0 {
            rows = rows.child(
                div()
                    .py_4()
                    .text_center()
                    .text_size(px(11.))
                    .text_color(rgb(palette.text_dimmed))
                    .child(if snapshot.history_pending {
                        t!("ai.historyLoading")
                    } else if total_count == 0 {
                        t!("ai.noHistory")
                    } else {
                        t!("ai.noHistoryMatches")
                    }),
            );
        } else {
            for (group, sessions) in grouped {
                if sessions.is_empty() {
                    continue;
                }
                rows = rows.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_size(px(10.))
                        .font_weight(FontWeight(700.))
                        .text_color(rgb(palette.text_dimmed))
                        .child(group),
                );
                for (date, date_sessions) in group_ai_sessions_by_date(&sessions) {
                    rows = rows.child(
                        div()
                            .px_2()
                            .text_size(px(9.))
                            .text_color(rgb(palette.text_dimmed))
                            .child(t!(date.label_key())),
                    );
                    for session in date_sessions {
                        let session_id = session.id.clone();
                        let delete_id = session.id.clone();
                        let active = snapshot.current_ai_session_id == session.id;
                        let occupied = snapshot
                            .history_running_ids
                            .iter()
                            .any(|id| id == &session.id);
                        let open_disabled =
                            snapshot.history_pending || snapshot.running || occupied;
                        let delete_disabled = snapshot.history_pending || occupied;
                        rows = rows.child(
                            div()
                                .id(SharedString::from(format!("ai-session-{}", session.id)))
                                .debug_selector({
                                    let id = session.id.clone();
                                    move || format!("ai-history-session-{id}")
                                })
                                .h(px(32.))
                                .px_2()
                                .rounded_md()
                                .flex()
                                .items_center()
                                .gap_1()
                                .bg(if active {
                                    rgb(palette.hover)
                                } else {
                                    rgba(0x00000000)
                                })
                                .hover(move |this| this.bg(rgb(palette.surface_elevated)))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "ai-session-open-{}",
                                            session.id
                                        )))
                                        .min_w_0()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(rgb(palette.text))
                                        .overflow_hidden()
                                        .when(!open_disabled, |this| this.cursor_pointer())
                                        .when(open_disabled, |this| this.opacity(0.5))
                                        .child(truncate_preview(&session.title, 28))
                                        .on_click(cx.listener(move |panel, _, _, cx| {
                                            if open_disabled {
                                                return;
                                            }
                                            let session_id = session_id.clone();
                                            panel.with_app(cx, move |app, cx| {
                                                app.load_ai_session_messages(session_id, cx);
                                            });
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.))
                                        .text_color(rgb(palette.text_dimmed))
                                        .child(format!(
                                            "{}{}",
                                            agent_kind_label(&session.agent_kind),
                                            if occupied {
                                                t!("ai.historyInUse")
                                            } else if session.external_session_id.is_some() {
                                                t!("ai.historyResume")
                                            } else {
                                                "".into()
                                            }
                                        )),
                                )
                                .child(svg_icon_button(
                                    format!("ai-session-delete-{}", session.id),
                                    "icons/fe/delete.svg",
                                    14.,
                                    palette,
                                    cx.listener(move |panel, _, window, cx| {
                                        if delete_disabled {
                                            return;
                                        }
                                        let delete_id = delete_id.clone();
                                        panel.with_app(cx, move |app, cx| {
                                            app.open_ai_delete_history_confirm(
                                                delete_id, window, cx,
                                            );
                                        });
                                    }),
                                )),
                        );
                    }
                }
            }
        }

        div()
            .id(SharedString::from("ai-history-popover"))
            .debug_selector(|| "ai-history-popover".to_string())
            .absolute()
            .top(px(4.))
            .left(px(8.))
            .right(px(8.))
            .h(px(if filtered_count == 0 { 156. } else { 352. }))
            .max_h(gpui::relative(0.95))
            .rounded_md()
            .border_1()
            .border_color(rgb(palette.border))
            .bg(snapshot.chrome.surface)
            .shadow_lg()
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .when_some(search_input, |this, search_input| {
                this.child(
                    div()
                        .flex_none()
                        .p_2()
                        .border_b_1()
                        .border_color(rgb(palette.border))
                        .child(search_input),
                )
            })
            .child(
                div()
                    .h(px(32.))
                    .flex_none()
                    .px_2()
                    .border_b_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight(700.))
                            .text_color(rgb(palette.text))
                            .child(if snapshot.history_pending {
                                t!("ai.historyLoading")
                            } else {
                                t!("ai.history")
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from("ai-history-clear-all"))
                            .h(px(22.))
                            .px_2()
                            .rounded_sm()
                            .flex()
                            .items_center()
                            .text_size(px(11.))
                            .text_color(if snapshot.history_actions_disabled {
                                rgb(palette.border)
                            } else {
                                rgb(palette.text_muted)
                            })
                            .when(!snapshot.history_actions_disabled, |this| {
                                this.cursor_pointer().hover(move |this| {
                                    this.bg(rgb(palette.surface_elevated))
                                        .text_color(rgb(palette.text))
                                })
                            })
                            .on_click(cx.listener(|panel, _, window, cx| {
                                panel.with_app(cx, |app, cx| {
                                    if app.ai.history_actions_are_disabled() {
                                        return;
                                    }
                                    app.open_ai_clear_history_confirm(window, cx);
                                });
                            }))
                            .child(t!("ai.clearHistory")),
                    ),
            )
            .when_some(snapshot.history_error.as_ref(), |this, error| {
                this.child(
                    div()
                        .flex_none()
                        .max_h(px(42.))
                        .overflow_hidden()
                        .px_2()
                        .py_1()
                        .text_size(px(11.))
                        .text_color(rgb(palette.danger))
                        .child(format!("{}: {error}", t!("ai.historyLoadFailed"))),
                )
            })
            .child(
                div()
                    .id(SharedString::from("ai-history-scroll"))
                    .debug_selector(|| "ai-history-viewport".to_string())
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .id("ai-history-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.history_scroll)
                            .child(rows),
                    )
                    .vertical_scrollbar(&self.history_scroll),
            )
    }

    fn ai_message_context_menu_overlay(
        &self,
        snapshot: &AiPanelSnapshot,
        state: AiMessageMenuState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let palette = snapshot.chrome.palette;
        let quote_text = state.text.clone();
        let copy_text = state.text.clone();
        let (menu_x, menu_y, menu_max_h) = ai_message_menu_position(
            f32::from(state.x),
            f32::from(state.y),
            128.,
            64.,
            snapshot.chrome.viewport_width,
            snapshot.chrome.viewport_height,
        );
        full_window_input_layer("ai-message-context-menu-overlay")
            .on_click(cx.listener(|panel, _, _, cx| {
                panel.with_app(cx, |app, cx| {
                    app.close_ai_message_menu(cx);
                });
            }))
            .child(
                div()
                    .id(SharedString::from("ai-message-context-menu"))
                    .absolute()
                    .top(px(menu_y))
                    .left(px(menu_x))
                    .w(px(128.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .bg(snapshot.chrome.surface)
                    .shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .max_h(px(menu_max_h))
                            .overflow_y_scrollbar()
                            .py_1()
                            .flex()
                            .flex_col()
                            .child(ai_message_menu_button(
                                palette,
                                "ai-message-menu-quote",
                                "icons/quote.svg",
                                t!("ai.quote"),
                                cx.listener(move |panel, _, _, cx| {
                                    let quote_text = quote_text.clone();
                                    panel.with_app(cx, move |app, cx| {
                                        app.quote_ai_message_text(quote_text, cx);
                                    });
                                }),
                            ))
                            .child(ai_message_menu_button(
                                palette,
                                "ai-message-menu-copy",
                                "icons/copy.svg",
                                t!("ai.copy"),
                                cx.listener(move |panel, _, _, cx| {
                                    let copy_text = copy_text.clone();
                                    panel.with_app(cx, move |app, cx| {
                                        app.copy_ai_message_text(copy_text, cx);
                                    });
                                }),
                            )),
                    ),
            )
    }
}

impl gpui::Render for AiPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| !snapshot.history_open)
            && let Some(app) = self.app.upgrade()
            && !app.read(cx).ai.history_is_open()
            && app.read(cx).ai.history_has_restore_focus()
        {
            let search_focused = app
                .read(cx)
                .existing_text_input("ai.history-search")
                .is_some_and(|field| {
                    let input = field.read(cx);
                    input.focus_handle().contains_focused(window, cx)
                        || input
                            .component_focus_handle(cx)
                            .contains_focused(window, cx)
                });
            let focus = app.update(cx, |app, _| {
                let focus = app.ai.take_history_focus();
                app.forget_text_inputs("ai.history-search");
                focus
            });
            if search_focused && let Some(focus) = focus {
                window.focus(&focus, cx);
            }
        }
        if let Some(snapshot) = &self.snapshot
            && let Some(view) = snapshot.native_run.as_ref().filter(|view| {
                view.status == nyaterm_core::ai::harness::AgentRunStatus::WaitingForUser
            })
            && let (Some(call_id), Some(input)) =
                (&view.call_id, snapshot.native_answer_inputs.first())
        {
            let key = (view.run_id.clone(), call_id.clone());
            if self.focused_question.as_ref() != Some(&key) {
                if let Some(index) = self.transcript_rows.iter().position(|row| {
                    matches!(row, AiTranscriptRow::NativeRun { run_id, .. } if run_id == &view.run_id)
                }) {
                    self.transcript_scroll.update(cx, |scroll, cx| {
                        scroll.scroll_to_item(index, cx);
                    });
                }
                window.focus(&input.read(cx).focus_handle(), cx);
                self.focused_question = Some(key);
            }
        }
        // The virtual list detects width changes itself. Font metrics and
        // translated labels also invalidate heights of off-screen rows.
        let text_style = (
            window.text_style(),
            window.rem_size(),
            rust_i18n::locale().to_string(),
        );
        if self.transcript_text_style.as_ref() != Some(&text_style) {
            self.transcript_text_style = Some(text_style);
            self.transcript_scroll
                .update(cx, |state, cx| state.remeasure(cx));
        }
        #[cfg(test)]
        {
            self.paint_count += 1;
        }
        self.render_panel(cx)
    }
}

impl NyaTermApp {
    pub(in crate::features) fn ai_header_presentation(&self) -> AiHeaderPresentation {
        let selected_model_id = self.ai_selected_model_id();
        let mut model_label = selected_model_id
            .as_deref()
            .and_then(|model_id| {
                self.ai
                    .settings_config()
                    .models
                    .iter()
                    .find(|model| model.id == model_id)
                    .map(|model| truncate_preview(&model.name, 28))
            })
            .unwrap_or_else(|| t!("ai.notConfigured").to_string());
        if self.ai.chat_run_mode() == AiMode::Agent {
            match self.ai.chat_agent_kind() {
                AiAgentKind::Codex => {
                    model_label = self
                        .ai
                        .settings_config()
                        .codex
                        .default_model
                        .clone()
                        .filter(|model| !model.trim().is_empty())
                        .unwrap_or_else(|| "Codex".to_string())
                }
                AiAgentKind::ClaudeCode => {
                    model_label = self
                        .ai
                        .settings_config()
                        .claude_code
                        .default_model
                        .clone()
                        .filter(|model| !model.trim().is_empty())
                        .unwrap_or_else(|| "Claude Code".to_string())
                }
                AiAgentKind::Nyaterm => {}
            }
        }
        if let Some(active_id) = self.session.active_id() {
            let label = self
                .session
                .display_name(active_id)
                .unwrap_or_else(|| short_id(active_id).to_string());
            let targets = self.ai_effective_target_session_ids();
            let count = targets.iter().filter(|id| id.as_str() != active_id).count();
            model_label = if count > 0 {
                t!("ai.panelMetaMultiTarget", target = label, count = count).to_string()
            } else {
                label
            };
        }
        AiHeaderPresentation {
            running: self.ai.chat_or_agent_is_running(),
            selected_model_id,
            model_label,
            execution_mode: self
                .ai
                .settings_config()
                .agent_command_execution_mode
                .clone(),
        }
    }

    pub(in crate::features) fn notify_root_if_ai_header_changed(
        &self,
        before: AiHeaderPresentation,
        cx: &mut Context<Self>,
    ) -> bool {
        if before == self.ai_header_presentation() {
            return false;
        }
        cx.notify();
        true
    }

    pub(in crate::features) fn dismiss_ai_detected_error(&mut self, cx: &mut Context<Self>) {
        self.ai.dismiss_detected_error();
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn close_ai_message_menu(&mut self, cx: &mut Context<Self>) {
        self.ai.close_message_menu();
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn quote_ai_message_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        self.ai.quote_message(text);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn copy_ai_message_text(
        &mut self,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let value = text.trim().to_string();
        let copied = !value.is_empty();
        if copied {
            cx.write_to_clipboard(ClipboardItem::new_string(value));
        }
        self.ai.finish_copy_message(copied);
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn clear_ai_quote(&mut self, cx: &mut Context<Self>) {
        self.ai.clear_quote();
        self.defer_ai_panel_snapshot_flush(cx);
    }

    pub(in crate::features) fn analyze_ai_detected_error(
        &mut self,
        detected: AiDetectedErrorState,
        cx: &mut Context<Self>,
    ) {
        if self.ai.chat_or_agent_is_running() {
            self.ai
                .set_chat_response_preview("AI request already running");
            self.ai.set_panel_status("AI request already running");
            self.defer_ai_panel_snapshot_flush(cx);
            return;
        }
        let mut context = self.ai_terminal_context_for_session(Some(&detected.session_id));
        context.selected_text = detected.output.clone();
        let request = AiPreparedRequest {
            action: AiAction::AnalyzeError,
            context,
            source_label: "Detected terminal error".to_string(),
        };
        self.ai
            .prepare_detected_error_request(request, detected.session_id.clone());
        self.set_ai_prompt_draft("Analyze detected error", cx);
        self.start_ai_ask(cx);
    }

    fn open_ai_delete_history_confirm(
        &mut self,
        session_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ai.history_is_pending() || self.ai.ai_session_is_running(&session_id) {
            return;
        }
        self.open_confirm_dialog(
            (
                t!("ai.deleteHistoryTitle").to_string(),
                t!("ai.deleteHistoryDesc").to_string(),
                t!("ai.deleteSession").to_string(),
                true,
                move |app, _, cx| {
                    if app.ai.history_is_pending() || app.ai.ai_session_is_running(&session_id) {
                        return false;
                    }
                    app.delete_ai_session(session_id.clone(), cx);
                    true
                },
            ),
            window,
            cx,
        );
        cx.notify();
    }

    pub(in crate::features) fn open_ai_clear_history_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ai.request_history_clear_confirm() {
            return;
        }
        self.open_confirm_dialog(
            (
                t!("ai.clearHistoryTitle").to_string(),
                t!("ai.clearHistoryDesc").to_string(),
                t!("ai.clearHistory").to_string(),
                true,
                |app, _, cx| app.confirm_ai_clear_history(cx),
            ),
            window,
            cx,
        );
        self.defer_ai_panel_snapshot_flush(cx);
        cx.notify();
    }

    pub(in crate::features) fn confirm_ai_clear_history(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.ai.confirm_history_clear() {
            return false;
        }
        self.clear_all_ai_history(cx);
        true
    }

    pub(in crate::features) fn open_ai_auto_execution_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ai.request_agent_auto_confirm();
        self.open_confirm_dialog(
            (
                t!("ai.autoExecutionConfirmTitle").to_string(),
                t!("ai.autoExecutionConfirmDesc").to_string(),
                t!("ai.enableAutoExecution").to_string(),
                true,
                |app, _, cx| app.confirm_ai_auto_execution(cx),
            ),
            window,
            cx,
        );
        self.defer_ai_panel_snapshot_flush(cx);
        cx.notify();
    }

    pub(in crate::features) fn confirm_ai_auto_execution(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let before = self.ai_header_presentation();
        if !self.ai.confirm_agent_auto_execution() {
            return false;
        }
        self.persist_ai_settings_now(cx);
        self.defer_ai_panel_snapshot_flush(cx);
        self.notify_root_if_ai_header_changed(before, cx);
        true
    }

    pub(in crate::features) fn defer_ai_panel_snapshot_flush(&mut self, cx: &mut Context<Self>) {
        if !self.ai.request_panel_refresh() {
            return;
        }
        self.defer_app_update(cx, |app, cx| {
            if !app.ai.take_panel_refresh_request() {
                return;
            }
            app.flush_ai_panel_snapshot(cx);
        });
    }

    pub(in crate::features) fn flush_ai_panel_snapshot(&mut self, cx: &mut Context<Self>) {
        self.ai.clear_panel_refresh_request();
        let snapshot = self.build_ai_panel_snapshot(cx);
        let panel = self.ai_panel.clone();
        panel.update(cx, |panel, cx| panel.set_snapshot(snapshot, cx));
    }

    fn ai_model_provider_presentation(
        &self,
        model: &AiModelConfigItem,
    ) -> (Option<AiProviderKind>, Option<Arc<RenderImage>>) {
        let credential = model.credential_id.as_ref().and_then(|id| {
            self.ai
                .settings_config()
                .provider_credentials
                .iter()
                .find(|credential| &credential.id == id)
        });
        (
            credential
                .map(|credential| credential.provider_kind.clone())
                .or_else(|| model.provider_kind.clone()),
            model
                .credential_id
                .as_ref()
                .and_then(|id| self.ai.provider_view().icons.get(id).cloned()),
        )
    }

    fn build_ai_panel_snapshot(&mut self, cx: &mut Context<Self>) -> AiPanelSnapshot {
        let palette = self.theme_palette();
        let enabled = self.ai.settings_config().enabled;
        let agent_mode = self.ai.chat_run_mode() == AiMode::Agent;
        let running = self.ai.chat_or_agent_is_running();
        let agent_kind = self.ai.chat_agent_kind();
        let external_agent = agent_mode && agent_kind != AiAgentKind::Nyaterm;
        let selected_model_id = self.ai_selected_model_id();
        let enabled_models: Arc<[AiModelConfigItem]> = self.ai_enabled_models().into();
        let selected_model_exists = selected_model_id
            .as_deref()
            .is_some_and(|model_id| enabled_models.iter().any(|model| model.id == model_id));
        let model_label = selected_model_id
            .as_deref()
            .and_then(|model_id| enabled_models.iter().find(|model| model.id == model_id))
            .map(|model| model.name.clone())
            .unwrap_or_else(|| t!("ai.notConfigured").to_string());
        let selected_model = enabled_models
            .iter()
            .find(|model| Some(&model.id) == selected_model_id.as_ref());
        let (selected_provider_kind, selected_provider_icon) = selected_model
            .map(|model| self.ai_model_provider_presentation(model))
            .unwrap_or_default();
        let reasoning_choices: Arc<[AiReasoningEffort]> =
            self.ai_filtered_reasoning_choices().into();
        let model_choices_vec = self.ai_filtered_model_choices();
        self.ai
            .clamp_discovery_index(reasoning_choices.len() + model_choices_vec.len());
        let model_choices = model_choices_vec
            .into_iter()
            .map(|(model, provider_label)| {
                let (provider_kind, provider_icon) = self.ai_model_provider_presentation(&model);
                AiModelChoice {
                    model,
                    provider_label,
                    provider_kind,
                    provider_icon,
                }
            })
            .collect::<Vec<_>>()
            .into();
        let target_session_ids = self.ai.chat_target_session_ids().to_vec();
        let target_sessions = target_session_ids
            .iter()
            .filter_map(|session_id| {
                self.session
                    .session_info(session_id)
                    .map(|session| AiTargetSession {
                        session_id: session_id.clone(),
                        label: self.session.display_name_by_info(&session),
                    })
            })
            .collect::<Vec<_>>()
            .into();
        let mention_candidates: Arc<[AiMentionCandidate]> = if self.ai.chat_mention_is_open() {
            self.ai_mention_candidates()
                .into_iter()
                .map(|session| AiMentionCandidate {
                    selected: target_session_ids
                        .iter()
                        .any(|session_id| session_id == &session.id),
                    kind: crate::features::formatting::session_kind_label(session.kind).to_string(),
                    label: self.session.display_name_by_info(&session),
                    session_id: session.id,
                })
                .collect::<Vec<_>>()
                .into()
        } else {
            Vec::<AiMentionCandidate>::new().into()
        };
        self.ai.clamp_chat_mention_index(mention_candidates.len());

        let prompt_placeholder = if !enabled {
            t!("ai.goToSettingsToEnable")
        } else {
            t!("ai.placeholder")
        };
        let prompt_draft = self.ai.chat_prompt_draft().to_string();
        self.ensure_text_input(
            "ai.chat.prompt",
            &prompt_draft,
            TextInputSetup::multi_line(prompt_placeholder.clone()).submit_on_enter(),
            cx,
        );
        let prompt_input = self
            .existing_text_input("ai.chat.prompt")
            .expect("AI prompt input was just built");

        prompt_input.update(cx, |input, cx| {
            input.set_disabled(running || !enabled, cx);
            input.set_placeholder(prompt_placeholder, cx);
        });

        let model_search_input = if self.ai.discovery_menu_is_open() {
            let query = self.ai.discovery_query().to_string();
            self.ensure_text_input(
                "ai.model-search",
                &query,
                TextInputSetup::placeholder(t!("ai.searchModels")),
                cx,
            );
            self.existing_text_input("ai.model-search")
        } else {
            None
        };
        let history_search_input = if self.ai.history_is_open() {
            let query = self.ai.history_query().to_string();
            self.ensure_text_input(
                "ai.history-search",
                &query,
                TextInputSetup::placeholder(t!("ai.historySearchPlaceholder")),
                cx,
            );
            self.existing_text_input("ai.history-search")
        } else {
            None
        };
        let native_run = self.ai.native_run_view();
        let mut native_answer_inputs = Vec::new();
        if let Some(view) = &native_run
            && view.status == nyaterm_core::ai::harness::AgentRunStatus::WaitingForUser
            && let Some(call_id) = &view.call_id
        {
            for (index, question) in view.questions.iter().enumerate() {
                let id = harness::answer_input_id(&view.run_id, call_id, index);
                self.ensure_text_input(
                    id.clone(),
                    view.answers
                        .get(&question.id)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    TextInputSetup::placeholder(t!("ai.harness.answerPlaceholder"))
                        .submit_on_enter(),
                    cx,
                );
                if let Some(input) = self.existing_text_input(&id) {
                    native_answer_inputs.push(input);
                }
            }
        }
        let (viewport_width, viewport_height) = self.shell.viewport_size();

        AiPanelSnapshot {
            index: Arc::default(),
            native_run,
            native_answer_inputs,
            ui_font_family: if self.settings.summary().ui_font_family.trim().is_empty() {
                crate::features::shell::gpui_ui_font_fallback().into()
            } else {
                self.gpui_ui_font().family.into()
            },
            chrome: AiPanelChrome {
                palette,
                transparent_surface: self.shell_transparent_color(palette.surface),
                transparent_section_header: self.shell_transparent_color(palette.section_header),
                surface: self.shell_surface_color(palette.surface),
                viewport_width,
                viewport_height,
            },
            enabled,
            agent_mode,
            running,
            agent_kind,
            reasoning_effort: self.ai.settings_config().default_reasoning_effort.clone(),
            reasoning_choices,
            codex_enabled: self.ai.settings_config().codex.enabled,
            claude_code_enabled: self.ai.settings_config().claude_code.enabled,
            external_agent,
            external_model_label: match self.ai.chat_agent_kind() {
                AiAgentKind::Codex => self
                    .ai
                    .settings_config()
                    .codex
                    .default_model
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "Codex".to_string()),
                AiAgentKind::ClaudeCode => self
                    .ai
                    .settings_config()
                    .claude_code
                    .default_model
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "Claude Code".to_string()),
                AiAgentKind::Nyaterm => String::new(),
            },
            selected_model_id,
            selected_model_exists,
            model_label,
            selected_provider_kind,
            selected_provider_icon,
            enabled_models,
            model_choices,
            discovery_menu_open: self.ai.discovery_menu_is_open(),
            discovery_index: self.ai.discovery_index(),
            prompt_draft,
            prompt_input,
            model_search_input,
            history_search_input,
            file_action_ready: self
                .ai
                .chat_prepared_request()
                .is_some_and(|request| request.action == AiAction::CustomFileAction),
            messages: self.ai.chat_snapshot_messages(),
            streaming_assistant_id: self.ai.chat_streaming_assistant_id().map(str::to_string),
            response_phase: self.ai.response_phase(),
            expanded_message_thoughts: self
                .ai
                .expanded_message_thoughts()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            expanded_command_details: self
                .ai
                .expanded_command_details()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            expanded_command_scripts: self
                .ai
                .expanded_command_scripts()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            agent_history_expanded: self.ai.agent_history_expanded(),
            expanded_execution_groups: self
                .ai
                .expanded_execution_groups()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            command_cards: self.ai.chat_command_cards().to_vec().into(),
            agent_steps: self
                .ai
                .agent_steps()
                .iter()
                .cloned()
                .map(|step| AiAgentStepPresentation {
                    thought_open: self.ai.agent_thought_is_expanded(step.step_index),
                    output_open: self.ai.agent_output_is_expanded(step.step_index),
                    step,
                })
                .collect::<Vec<_>>()
                .into(),
            target_sessions,
            mention_open: self.ai.chat_mention_is_open(),
            mention_index: self.ai.chat_mention_index(),
            mention_candidates,
            quoted_text: self.ai.chat_quote().map(str::to_string),
            detected_error: self.ai.panel_detected_error().cloned(),
            message_menu: self.ai.chat_message_menu().cloned(),
            history_open: self.ai.history_is_open(),
            history_query: self.ai.history_query().to_string(),
            history_sessions: self.ai.history_sessions().to_vec().into(),
            history_running_ids: self
                .ai
                .history_sessions()
                .iter()
                .filter(|session| self.ai.ai_session_is_running(&session.id))
                .map(|session| session.id.clone())
                .collect::<Vec<_>>()
                .into(),
            current_ai_session_id: self.ai.chat_session_id().to_string(),
            owner_terminal_id: self.session.active_id().map(str::to_string),
            owner_connection_id: self
                .session
                .active_id()
                .and_then(|id| self.session.metadata(id))
                .and_then(|metadata| metadata.source_connection_id.clone()),
            history_pending: self.ai.history_is_pending(),
            history_error: self.ai.history_error().map(str::to_string),
            history_actions_disabled: self.ai.history_actions_are_disabled(),
            execution_menu_open: self.ai.panel_execution_menu_is_open(),
            command_execution_mode: self
                .ai
                .settings_config()
                .agent_command_execution_mode
                .clone(),
            background_execution_enabled: self
                .ai
                .settings_config()
                .agent_background_execution_enabled,
        }
    }

    pub(in crate::features) fn sync_ai_active_scope(&mut self, cx: &mut Context<Self>) {
        let scope = self
            .session
            .active_id()
            .map(|session_id| format!("terminal:{session_id}"))
            .unwrap_or_else(|| "unbound:".to_string());
        if self.ai.switch_visible_scope(&scope) {
            let draft = self.ai.chat_prompt_draft().to_string();
            self.reset_text_input("ai.chat.prompt", &draft, cx);
        }
    }
}

fn agent_kind_label(kind: &AiAgentKind) -> &'static str {
    match kind {
        AiAgentKind::Nyaterm => "NyaTerm",
        AiAgentKind::Codex => "Codex",
        AiAgentKind::ClaudeCode => "Claude",
    }
}

fn ai_agent_status_badge(snapshot: &AiPanelSnapshot) -> impl IntoElement {
    let palette = snapshot.chrome.palette;
    div()
        .min_w_0()
        .flex_1()
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(rgb(palette.border))
        .bg(rgb(palette.input))
        .text_size(px(11.))
        .text_color(rgb(palette.text_muted))
        .child(
            div()
                .min_w_0()
                .text_ellipsis()
                .child(snapshot.external_model_label.clone()),
        )
}

fn ai_menu_heading(palette: ThemePalette, label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .h(px(24.))
        .flex_none()
        .px_2()
        .flex()
        .items_center()
        .text_size(px(10.))
        .text_color(rgb(palette.text_muted))
        .child(label.into())
}

fn ai_choice_check(palette: ThemePalette, selected: bool) -> impl IntoElement {
    div().size(px(14.)).flex_none().when(selected, |this| {
        this.child(
            svg()
                .size(px(13.))
                .path("icons/check.svg")
                .text_color(rgb(palette.link)),
        )
    })
}

fn ai_model_provider_badge(
    palette: ThemePalette,
    kind: Option<&AiProviderKind>,
    image: Option<&Arc<RenderImage>>,
) -> gpui::AnyElement {
    if let Some(image) = image {
        return img(image.clone())
            .size(px(16.))
            .flex_none()
            .rounded_full()
            .into_any_element();
    }
    let path = match kind {
        Some(AiProviderKind::Openai) => "icons/settings/openai.svg",
        Some(AiProviderKind::Anthropic) => "icons/settings/anthropic.svg",
        Some(AiProviderKind::Gemini) => "icons/settings/gemini.svg",
        Some(AiProviderKind::Deepseek) => "icons/settings/deepseek.svg",
        Some(AiProviderKind::Xai) => "icons/settings/xai.svg",
        Some(AiProviderKind::Zai) => "icons/settings/zai.svg",
        Some(AiProviderKind::Ollama) => "icons/settings/ollama.svg",
        Some(AiProviderKind::Mimo) => "icons/settings/mimo.svg",
        _ => "icons/settings/cloud.svg",
    };
    div()
        .size(px(16.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when(kind == Some(&AiProviderKind::Cohere), |this| {
            this.text_size(px(10.))
                .text_color(rgb(palette.accent))
                .child("C")
        })
        .when(kind != Some(&AiProviderKind::Cohere), |this| {
            this.child(
                svg()
                    .size(px(14.))
                    .path(path)
                    .text_color(rgb(palette.accent)),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    mod history_and_streaming;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Instant;

    use super::{AiAgentStepPresentation, AiMentionCandidate, AiPanel, AiPanelSnapshot};
    use crate::features::ai::panel::transcript::{AiTranscriptRow, AiTranscriptUpdate};
    use crate::features::runtime_jobs::{AiAgentStepStatus, AiAgentStepView};
    use gpui::{
        AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _,
        TestAppContext, VisualTestContext, div, px,
    };
    use nyaterm_core::{
        AgentCommandExecutionMode, AiCommandCard, AiMessage, AiMessageRole, AiMode,
        AiModelConfigItem, AiModelSource, AiProviderKind, AiSettings, AppRuntime, RuntimeMode,
    };
    use nyaterm_ui::NyaInputEvent;

    use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
    use crate::features::{NyaTermApp, runtime_jobs::AiChatJobOutput};
    use crate::test_support::TestConfigDir;

    fn app(cx: &mut TestAppContext, root: &Path) -> Entity<NyaTermApp> {
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.to_path_buf(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let stores = UiStoreHandles {
            startup_restore: cx.new(|_| StartupRestoreStore::default()),
            overlays: cx.new(|_| OverlayStore::default()),
        };
        cx.new(|cx| NyaTermApp::new(runtime, stores, cx))
    }

    struct AppHost {
        app: Entity<NyaTermApp>,
    }

    impl Render for AppHost {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let app = self.app.read(cx);
            div()
                .w(px(360.))
                .h(px(720.))
                .flex()
                .gap_1()
                .child(
                    div().flex_1().min_h_0().overflow_hidden().child(
                        app.ai_panel
                            .clone()
                            .cached(crate::features::layout::cached_panel_style()),
                    ),
                )
                .child(
                    div().w(px(260.)).min_h_0().overflow_hidden().child(
                        app.connection_panel
                            .clone()
                            .cached(crate::features::layout::cached_panel_style()),
                    ),
                )
                .child(
                    div().w(px(260.)).min_h_0().overflow_hidden().child(
                        app.transfer_panel
                            .clone()
                            .cached(crate::features::layout::cached_panel_style()),
                    ),
                )
                .child(
                    div().w(px(260.)).min_h_0().overflow_hidden().child(
                        app.settings_panel
                            .clone()
                            .cached(crate::features::layout::cached_panel_style()),
                    ),
                )
        }
    }

    fn hosted<'a>(
        cx: &'a mut TestAppContext,
        root: &Path,
    ) -> (Entity<NyaTermApp>, &'a mut VisualTestContext) {
        let app = app(cx, root);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            app.flush_ai_panel_snapshot(cx);
            app.flush_connection_panel_snapshot(cx);
            app.flush_transfer_panel_snapshot(cx);
            app.flush_settings_panel_snapshots(cx);
        });
        let host_app = app.clone();
        let (_, vcx) = cx.add_window_view(move |_, _| AppHost {
            app: host_app.clone(),
        });
        let vcx: &mut VisualTestContext = vcx;
        vcx.run_until_parked();
        for _ in 0..3 {
            vcx.update(|window, cx| {
                app.update(cx, |_, cx| cx.notify());
                _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        (app, vcx)
    }

    fn draw(app: &Entity<NyaTermApp>, vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| {
            app.update(cx, |_, cx| cx.notify());
            _ = window.draw(cx);
        });
        vcx.run_until_parked();
    }

    fn ai_paints(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).ai_panel.read(cx).paint_count()
    }

    fn ai_snapshot_sets(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).ai_panel.read(cx).snapshot_set_count()
    }

    fn connection_paints(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).connection_panel.read(cx).paint_count()
    }

    fn transfer_paints(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).transfer_panel.read(cx).paint_count()
    }

    fn settings_paints(app: &Entity<NyaTermApp>, cx: &mut gpui::App) -> usize {
        app.read(cx).settings_panel.read(cx).paint_count()
    }

    #[test]
    fn detected_terminal_error_refreshes_ai_panel_only() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx, test_dir.path());
        let before_snapshots = vcx.update(|_, cx| ai_snapshot_sets(&app, cx));
        let before_ai_paints = vcx.update(|_, cx| ai_paints(&app, cx));
        let before_connection_paints = vcx.update(|_, cx| connection_paints(&app, cx));
        let before_transfer_paints = vcx.update(|_, cx| transfer_paints(&app, cx));
        let before_settings_paints = vcx.update(|_, cx| settings_paints(&app, cx));

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                assert!(app.ai.note_detected_error(
                    "session-a".to_string(),
                    "permission denied".to_string(),
                    Instant::now(),
                ));
                app.defer_ai_panel_snapshot_flush(cx);
            });
        });
        vcx.run_until_parked();

        assert_eq!(
            vcx.update(|_, cx| ai_snapshot_sets(&app, cx)),
            before_snapshots + 1,
            "detected terminal errors should rebuild the AiPanel snapshot"
        );
        draw(&app, vcx);
        assert!(
            vcx.update(|_, cx| ai_paints(&app, cx)) > before_ai_paints,
            "the AiPanel should repaint after its snapshot changes"
        );
        assert_eq!(
            vcx.update(|_, cx| connection_paints(&app, cx)),
            before_connection_paints,
            "AI-owned refreshes must not repaint the connections panel"
        );
        assert_eq!(
            vcx.update(|_, cx| transfer_paints(&app, cx)),
            before_transfer_paints,
            "AI-owned refreshes must not repaint the transfer panel"
        );
        assert_eq!(
            vcx.update(|_, cx| settings_paints(&app, cx)),
            before_settings_paints,
            "AI-owned refreshes must not repaint the settings panel"
        );
    }

    #[test]
    fn repeated_ai_refresh_requests_coalesce() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx, test_dir.path());
        let before = vcx.update(|_, cx| ai_snapshot_sets(&app, cx));

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.defer_ai_panel_snapshot_flush(cx);
                app.defer_ai_panel_snapshot_flush(cx);
                app.defer_ai_panel_snapshot_flush(cx);
            });
        });
        vcx.run_until_parked();

        let after = vcx.update(|_, cx| ai_snapshot_sets(&app, cx));
        assert_eq!(
            after,
            before + 1,
            "same-cycle refresh requests should build/set one snapshot"
        );

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.defer_ai_panel_snapshot_flush(cx);
            });
        });
        vcx.run_until_parked();

        assert_eq!(
            vcx.update(|_, cx| ai_snapshot_sets(&app, cx)),
            after + 1,
            "a completed flush must not lock out the next refresh request"
        );

        let after_single = vcx.update(|_, cx| ai_snapshot_sets(&app, cx));
        vcx.update(|_, cx| {
            let panel = app.read(cx).ai_panel.clone();
            panel.update(cx, |panel, cx| {
                panel.with_app(cx, |app, cx| {
                    app.defer_ai_panel_snapshot_flush(cx);
                });
            });
        });
        vcx.run_until_parked();

        assert_eq!(
            vcx.update(|_, cx| ai_snapshot_sets(&app, cx)),
            after_single + 1,
            "with_app fallback plus an explicit refresh should still coalesce"
        );
    }

    #[test]
    fn ai_header_running_transition_notifies_root() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let app = app(&mut cx, test_dir.path());
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);

            let idle = app.ai_header_presentation();
            assert!(!idle.running);
            let launch = app
                .ai
                .begin_chat_request("inspect".to_string(), AiMode::Ask, None);
            let running = app.ai_header_presentation();
            assert!(running.running);
            assert_ne!(idle, running, "idle -> running should move the header");

            assert!(app.ai.apply_chat_delta(launch.job_id, "hello", None));
            assert_eq!(
                app.ai_header_presentation(),
                running,
                "ordinary streaming deltas must not move the root header projection"
            );

            app.ai
                .finish_chat_job(
                    launch.job_id,
                    launch.session_id,
                    Ok(AiChatJobOutput {
                        native_call: None,
                        mode: AiMode::Ask,
                        text: "done".to_string(),
                        reasoning: None,
                        command_cards: Vec::new(),
                        auto_execute_first: false,
                        approval_note: None,
                    }),
                )
                .expect("matching job should finish");
            let finished = app.ai_header_presentation();
            assert!(!finished.running);
            assert_ne!(running, finished, "running -> idle should move the header");

            let cancel_launch =
                app.ai
                    .begin_chat_request("cancel me".to_string(), AiMode::Ask, None);
            let cancel_running = app.ai_header_presentation();
            assert!(cancel_running.running);
            app.ai.cancel_chat_and_agent();
            assert!(
                cancel_launch
                    .cancel
                    .load(std::sync::atomic::Ordering::Relaxed)
            );
            let cancelled = app.ai_header_presentation();
            assert!(!cancelled.running);
            assert_ne!(
                cancel_running, cancelled,
                "cancel should move running back to idle"
            );

            let execution_before = app.ai_header_presentation();
            app.ai
                .set_settings_command_mode(AgentCommandExecutionMode::Auto);
            let execution_after = app.ai_header_presentation();
            assert_ne!(
                execution_before, execution_after,
                "execution mode is part of the root header projection"
            );

            let settings = AiSettings {
                models: vec![
                    AiModelConfigItem {
                        supported_reasoning_efforts: None,
                        backend: Default::default(),
                        id: "openai:model-a".to_string(),
                        name: "Model A".to_string(),
                        provider_kind: Some(AiProviderKind::Openai),
                        credential_id: None,
                        enabled: true,
                        source: AiModelSource::Manual,
                        last_seen_at: None,
                    },
                    AiModelConfigItem {
                        supported_reasoning_efforts: None,
                        backend: Default::default(),
                        id: "openai:model-b".to_string(),
                        name: "Model B".to_string(),
                        provider_kind: Some(AiProviderKind::Openai),
                        credential_id: None,
                        enabled: true,
                        source: AiModelSource::Manual,
                        last_seen_at: None,
                    },
                ],
                default_model_id: Some("openai:model-a".to_string()),
                ..AiSettings::default()
            };
            app.ai.replace_settings_config(settings, true);
            let model_before = app.ai_header_presentation();
            app.ai.set_settings_default_model("openai:model-b");
            let model_after = app.ai_header_presentation();
            assert_ne!(
                model_before, model_after,
                "selected model is part of the root header projection"
            );
            assert_eq!(model_after.model_label, "Model B");
        });
    }

    #[test]
    fn unrelated_app_notify_does_not_repaint_cached_ai_panel() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx, test_dir.path());
        let before = vcx.update(|_, cx| ai_paints(&app, cx));
        assert!(
            before > 0,
            "the panel must have painted at least once, or this proves nothing"
        );

        for _ in 0..5 {
            draw(&app, vcx);
        }

        assert_eq!(
            vcx.update(|_, cx| ai_paints(&app, cx)),
            before,
            "unrelated app notifies must not repaint the cached AI panel"
        );
    }

    #[test]
    fn streaming_delta_repaints_ai_panel_without_repainting_sibling_panels() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx, test_dir.path());

        let before = vcx.update(|_, cx| {
            (
                ai_paints(&app, cx),
                connection_paints(&app, cx),
                transfer_paints(&app, cx),
                settings_paints(&app, cx),
            )
        });

        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                let launch = app
                    .ai
                    .begin_chat_request("inspect".to_string(), AiMode::Ask, None);
                assert!(app.ai.apply_chat_delta(launch.job_id, "hello", None));
                app.flush_ai_panel_snapshot(cx);
            });
        });
        vcx.update(|window, cx| {
            _ = window.draw(cx);
        });
        vcx.run_until_parked();

        let after = vcx.update(|_, cx| {
            (
                ai_paints(&app, cx),
                connection_paints(&app, cx),
                transfer_paints(&app, cx),
                settings_paints(&app, cx),
            )
        });
        assert!(after.0 > before.0, "streaming delta must repaint AI panel");
        assert_eq!(after.1, before.1, "connections panel must stay cached");
        assert_eq!(after.2, before.2, "transfers panel must stay cached");
        assert_eq!(after.3, before.3, "settings panel must stay cached");
    }

    #[test]
    fn prompt_subscription_refreshes_snapshot_before_next_paint() {
        let test_dir = TestConfigDir::new("nyaterm-ai-panel");
        let mut cx = TestAppContext::single();
        let (app, vcx) = hosted(&mut cx, test_dir.path());
        let prompt_input = vcx.update(|_, cx| {
            app.read(cx)
                .ai_panel
                .read(cx)
                .snapshot()
                .expect("hosted panel has snapshot")
                .prompt_input
                .clone()
        });

        vcx.update(|_, cx| {
            prompt_input.update(cx, |_, cx| {
                cx.emit(NyaInputEvent::Changed("explain status".to_string()));
            });
            assert_eq!(
                app.read(cx)
                    .ai_panel
                    .read(cx)
                    .snapshot()
                    .expect("snapshot remains available")
                    .prompt_draft,
                "",
                "the snapshot must wait for the deferred input flush"
            );
        });
        vcx.run_until_parked();

        vcx.update(|window, cx| {
            let snapshot = app.read(cx).ai_panel.read(cx).snapshot().cloned();
            assert_eq!(
                snapshot.expect("deferred flush ran").prompt_draft,
                "explain status"
            );
            _ = window.draw(cx);
        });
    }

    #[test]
    fn message_menu_position_stays_inside_viewport() {
        assert_eq!(
            super::components::ai_message_menu_position(1240., 780., 128., 64., 1280., 800.),
            (1144., 728., 784.)
        );
        assert_eq!(
            super::components::ai_message_menu_position(240., 180., 128., 64., 200., 120.),
            (64., 48., 104.)
        );
    }
    struct CompactAiHost {
        panel: Entity<AiPanel>,
        height: f32,
    }

    impl Render for CompactAiHost {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            div()
                .w(px(320.))
                .h(px(self.height))
                .child(self.panel.clone())
        }
    }

    fn compact_host<'a>(
        cx: &'a mut TestAppContext,
        root: &Path,
    ) -> (Entity<NyaTermApp>, &'a mut VisualTestContext) {
        let app = app(cx, root);
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            let mut settings = app.ai.settings_config_cloned();
            settings.enabled = true;
            settings.models = vec![AiModelConfigItem {
                id: "fixture".to_string(),
                name: "A long model name that must truncate in a narrow panel".to_string(),
                provider_kind: Some(AiProviderKind::OpenaiCompatible),
                credential_id: None,
                enabled: true,
                source: AiModelSource::Manual,
                backend: Default::default(),
                last_seen_at: None,
                supported_reasoning_efforts: None,
            }];
            settings.default_model_id = Some("fixture".to_string());
            app.ai.replace_settings_config(settings, false);
            app.flush_ai_panel_snapshot(cx);
        });
        let panel = app.read_with(cx, |app, _| app.ai_panel.clone());
        let (_, vcx) = cx.add_window_view(move |_, _| CompactAiHost {
            panel,
            height: 800.,
        });
        let vcx: &mut VisualTestContext = vcx;
        compact_draw(&app, vcx);
        (app, vcx)
    }

    fn compact_draw(app: &Entity<NyaTermApp>, vcx: &mut VisualTestContext) {
        vcx.run_until_parked();
        for _ in 0..2 {
            vcx.update(|window, cx| {
                app.read(cx)
                    .ai_panel
                    .clone()
                    .update(cx, |_, cx| cx.notify());
                window.refresh();
                _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
    }

    fn edit_panel_snapshot(
        app: &Entity<NyaTermApp>,
        vcx: &mut VisualTestContext,
        edit: impl FnOnce(&mut AiPanelSnapshot),
    ) {
        vcx.update(|_, cx| {
            let panel = app.read(cx).ai_panel.clone();
            panel.update(cx, |panel, cx| {
                let mut snapshot = panel.snapshot().unwrap().clone();
                edit(&mut snapshot);
                panel.set_snapshot(snapshot, cx);
            });
        });
        compact_draw(app, vcx);
    }

    #[test]
    fn narrow_ai_panel_centers_empty_state_and_keeps_compact_controls_inside_composer() {
        let root = TestConfigDir::new("nyaterm-ai-compact");
        let mut cx = TestAppContext::single();
        let (_, vcx) = compact_host(&mut cx, root.path());
        let viewport = vcx.debug_bounds("ai-transcript-viewport").unwrap();
        let empty = vcx.debug_bounds("ai-empty-transcript").unwrap();
        assert!((empty.center().y - viewport.center().y).abs() < px(16.));
        let composer = vcx.debug_bounds("ai-composer").unwrap();
        let prompt = vcx.debug_bounds("ai-prompt").unwrap();
        let mode = vcx.debug_bounds("ai-mode-control").unwrap();
        let model = vcx.debug_bounds("ai-model-control").unwrap();
        let send = vcx.debug_bounds("ai-send-control").unwrap();
        assert!(
            composer.size.height <= px(122.),
            "composer should have just a textarea and one control row"
        );
        assert_eq!(prompt.size.height, px(64.));
        for control in [mode, model, send] {
            assert!(control.top() >= prompt.bottom());
            assert!(control.left() >= composer.left());
            assert!(control.right() <= composer.right());
            assert!((control.center().y - send.center().y).abs() <= px(2.));
        }
        assert!(mode.right() <= model.left());
        assert!(model.right() <= send.left());
    }

    #[test]
    fn mention_picker_reveals_keyboard_selection_beyond_eight_sessions() {
        let root = TestConfigDir::new("nyaterm-ai-mentions");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.mention_open = true;
            snapshot.mention_index = 10;
            snapshot.mention_candidates = (0..12)
                .map(|index| AiMentionCandidate {
                    session_id: format!("fixture-{index}"),
                    label: format!("Terminal {index}"),
                    kind: "Local".to_string(),
                    selected: false,
                })
                .collect::<Vec<_>>()
                .into();
        });
        let selected = vcx.debug_bounds("ai-mention-row-10").unwrap();
        let prompt = vcx.debug_bounds("ai-prompt").unwrap();
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).mention_scroll.clone());
        assert!(
            scroll.offset().y < px(0.),
            "offset={:?}, max={:?}, viewport={:?}, selected={selected:?}",
            scroll.offset(),
            scroll.max_offset(),
            scroll.bounds()
        );
        assert!(selected.top() >= scroll.bounds().top());
        assert!(selected.bottom() <= scroll.bounds().bottom());
        assert!(selected.bottom() <= prompt.top());
        assert!(vcx.debug_bounds("ai-mention-row-11").is_some());
    }

    fn transcript_message(index: usize) -> Arc<AiMessage> {
        Arc::new(AiMessage {
            id: format!("message-{index}"),
            session_id: "fixture".to_string(),
            role: AiMessageRole::User,
            content: "fixture output\n".repeat(6),
            created_at: "2026-10-02T00:00:00Z".to_string(),
            reasoning_content: None,
            command_cards: Vec::new(),
        })
    }

    #[test]
    fn growing_transcript_follows_bottom_but_preserves_a_readers_scroll_position() {
        let root = TestConfigDir::new("nyaterm-ai-scroll");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (0..24).map(transcript_message).collect::<Vec<_>>().into();
        });
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).transcript_scroll.clone());
        assert!(vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        assert!(vcx.debug_bounds("ai-message-message-23").is_some());
        assert!(vcx.debug_bounds("ai-message-message-0").is_none());
        vcx.update(|_, cx| scroll.update(cx, |state, cx| state.scroll_to_item(4, cx)));
        compact_draw(&app, vcx);
        let anchor = vcx.debug_bounds("ai-message-message-4").unwrap().top();
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (0..25).map(transcript_message).collect::<Vec<_>>().into();
        });
        assert!(!vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        assert_eq!(
            vcx.debug_bounds("ai-message-message-4").unwrap().top(),
            anchor
        );
        vcx.update(|_, cx| scroll.update(cx, |state, cx| state.scroll_to_end(cx)));
        compact_draw(&app, vcx);
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (0..26).map(transcript_message).collect::<Vec<_>>().into();
        });
        assert!(vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        assert!(vcx.debug_bounds("ai-message-message-25").is_some());
    }

    #[test]
    fn prepending_history_preserves_the_visible_message_anchor() {
        let root = TestConfigDir::new("nyaterm-ai-prepend");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (10..34).map(transcript_message).collect::<Vec<_>>().into();
        });
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).transcript_scroll.clone());
        vcx.update(|_, cx| scroll.update(cx, |state, cx| state.scroll_to_item(4, cx)));
        compact_draw(&app, vcx);
        let anchor = vcx.debug_bounds("ai-message-message-14").unwrap().top();
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (0..34).map(transcript_message).collect::<Vec<_>>().into();
        });
        assert_eq!(
            vcx.debug_bounds("ai-message-message-14").unwrap().top(),
            anchor
        );
        assert!(!vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
    }

    #[test]
    fn streaming_remeasures_the_growing_row_and_preserves_a_readers_anchor() {
        let root = TestConfigDir::new("nyaterm-ai-stream-measure");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut messages = (0..24).map(transcript_message).collect::<Vec<_>>();
            Arc::make_mut(&mut messages[23]).role = AiMessageRole::Assistant;
            snapshot.messages = messages.into();
            snapshot.streaming_assistant_id = Some("message-23".to_string());
        });
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).transcript_scroll.clone());
        let previous_height = vcx
            .debug_bounds("ai-message-message-23")
            .unwrap()
            .size
            .height;
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut messages = snapshot.messages.to_vec();
            Arc::make_mut(&mut messages[23])
                .content
                .push_str(&"streamed line\n".repeat(6));
            snapshot.messages = messages.into();
        });
        assert!(
            vcx.debug_bounds("ai-message-message-23")
                .unwrap()
                .size
                .height
                > previous_height
        );
        assert!(vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        vcx.update(|_, cx| scroll.update(cx, |state, cx| state.scroll_to_item(4, cx)));
        compact_draw(&app, vcx);
        let anchor = vcx.debug_bounds("ai-message-message-4").unwrap().top();
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut messages = snapshot.messages.to_vec();
            Arc::make_mut(&mut messages[23])
                .content
                .push_str(&"more streamed text\n".repeat(12));
            snapshot.messages = messages.into();
        });
        assert_eq!(
            vcx.debug_bounds("ai-message-message-4").unwrap().top(),
            anchor
        );
        assert!(!vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
    }

    #[test]
    fn switching_conversations_resumes_tail_following_even_with_the_same_message_ids() {
        let root = TestConfigDir::new("nyaterm-ai-scroll-session");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.messages = (0..24).map(transcript_message).collect::<Vec<_>>().into();
        });
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).transcript_scroll.clone());
        vcx.update(|_, cx| scroll.update(cx, |state, cx| state.scroll_to_item(4, cx)));
        compact_draw(&app, vcx);
        assert!(!vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.current_ai_session_id = "another-conversation".to_string();
        });
        assert!(vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        assert!(vcx.debug_bounds("ai-message-message-23").is_some());
    }

    #[test]
    fn mixed_transcript_preserves_card_indices_and_remeasures_changed_agent_details() {
        let root = TestConfigDir::new("nyaterm-ai-transcript-projection");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut previous =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        previous.messages = (0..2).map(transcript_message).collect::<Vec<_>>().into();
        previous.agent_steps = (0..20)
            .map(|step_index| AiAgentStepPresentation {
                step: AiAgentStepView {
                    kind: crate::features::ai::presentation::AiAgentStepKind::Command,
                    source_message_id: None,
                    command_card_id: None,
                    exit_code: None,
                    step_index,
                    status: AiAgentStepStatus::Completed,
                    title: "Fixture step".to_string(),
                    detail: String::new(),
                    thought: None,
                    command: None,
                    observation: Some("Fixture output".to_string()),
                },
                thought_open: false,
                output_open: false,
            })
            .collect::<Vec<_>>()
            .into();
        previous.command_cards = (0..10)
            .map(|index| AiCommandCard {
                id: format!("card-{index}"),
                title: "Fixture command".to_string(),
                command: "echo fixture".to_string(),
                explanation: String::new(),
                risk_level: None,
                risk_reason: None,
                expected_effect: String::new(),
                rollback: None,
                category: None,
                references: Vec::new(),
                target_terminal_session_id: None,
                target: None,
            })
            .collect::<Vec<_>>()
            .into();
        let old_rows = AiTranscriptRow::project(&previous);
        assert_eq!(old_rows.len(), 26);
        assert!(matches!(
            old_rows[2],
            AiTranscriptRow::AgentStep {
                index: 4,
                step_index: 4
            }
        ));
        assert!(matches!(
            old_rows[25],
            AiTranscriptRow::Command { index: 7, .. }
        ));

        let mut next = previous.clone();
        next.messages = (0..3).map(transcript_message).collect::<Vec<_>>().into();
        let mut steps = next.agent_steps.to_vec();
        steps[19].output_open = true;
        next.agent_steps = steps.into();
        let rows = AiTranscriptRow::project(&next);
        let update = AiTranscriptUpdate::between(Some(&previous), &next, &old_rows, &rows);
        assert_eq!(
            update.splice,
            Some((2..2, 1)),
            "insert before the retained agent and command rows"
        );
        assert_eq!(
            update.remeasure,
            vec![18..19],
            "expanded output changes only its own row height"
        );
        assert!(!update.reset);
    }

    fn chat_card(id: &str) -> AiCommandCard {
        AiCommandCard {
            id: id.into(),
            title: "Inspect resources".into(),
            command: "echo fixture".into(),
            explanation: String::new(),
            risk_level: Some(nyaterm_core::RiskLevel::Low),
            risk_reason: Some("Read only".into()),
            expected_effect: "Show resources".into(),
            rollback: None,
            category: None,
            references: Vec::new(),
            target_terminal_session_id: Some("terminal-a".into()),
            target: None,
        }
    }

    fn linked_command_step(status: AiAgentStepStatus) -> AiAgentStepPresentation {
        AiAgentStepPresentation {
            step: AiAgentStepView {
                step_index: 0,
                status,
                kind: crate::features::ai::presentation::AiAgentStepKind::Command,
                source_message_id: Some("message-0".into()),
                command_card_id: Some("agent-fixture".into()),
                exit_code: None,
                title: "Inspect resources".into(),
                detail: String::new(),
                thought: None,
                command: Some("echo fixture".into()),
                observation: None,
            },
            thought_open: false,
            output_open: false,
        }
    }

    #[test]
    fn transcript_deduplicates_ids_and_preserves_unrelated_commands_and_old_text() {
        use crate::features::ai::presentation::{AiAgentStepKind, AiCommandPhase};
        let root = TestConfigDir::new("nyaterm-ai-card-dedup");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut snapshot =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let mut message = transcript_message(0);
        let original = "Agent proposed `echo fixture`; legacy note";
        Arc::make_mut(&mut message).content = original.into();
        Arc::make_mut(&mut message).command_cards =
            vec![chat_card("agent-fixture"), chat_card("agent-fixture")];
        snapshot.messages = vec![message].into();
        snapshot.command_cards = vec![
            chat_card("agent-fixture"),
            chat_card("different-id"),
            chat_card("different-id"),
        ]
        .into();
        snapshot.agent_steps = vec![linked_command_step(AiAgentStepStatus::Running)].into();
        let rows = AiTranscriptRow::project(&snapshot);
        assert_eq!(rows.len(), 2);
        assert!(
            matches!(&rows[1], AiTranscriptRow::Command { id, index: 1 } if id == "different-id")
        );
        assert_eq!(snapshot.card_owner("agent-fixture"), Some("message-0"));
        assert_eq!(snapshot.messages[0].content, original);
        assert_eq!(
            snapshot.command_phase(&chat_card("agent-fixture")),
            AiCommandPhase::Running
        );
        snapshot.agent_steps = Vec::new().into();
        assert_eq!(
            snapshot.command_phase(&chat_card("agent-fixture")),
            AiCommandPhase::HistoryUnknown
        );
        let mut final_step = linked_command_step(AiAgentStepStatus::Completed);
        final_step.step.kind = AiAgentStepKind::FinalAnswer;
        final_step.step.command_card_id = None;
        snapshot.agent_steps = vec![final_step.clone()].into();
        assert_eq!(AiTranscriptRow::project(&snapshot).len(), 2);
        final_step.step.source_message_id = Some("unrelated-message".into());
        snapshot.agent_steps = vec![final_step].into();
        assert_eq!(
            AiTranscriptRow::project(&snapshot).len(),
            3,
            "unrelated steps must remain visible"
        );
    }

    #[test]
    fn linked_execution_changes_remeasure_only_the_owning_message() {
        let root = TestConfigDir::new("nyaterm-ai-linked-height");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut previous =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let mut messages = (0..3).map(transcript_message).collect::<Vec<_>>();
        Arc::make_mut(&mut messages[0]).command_cards = vec![chat_card("agent-fixture")];
        previous.messages = messages.into();
        previous.agent_steps = vec![linked_command_step(AiAgentStepStatus::Running)].into();
        let mut next = previous.clone();
        let mut steps = next.agent_steps.to_vec();
        steps[0].step.status = AiAgentStepStatus::Completed;
        steps[0].step.observation = Some("resource output".into());
        steps[0].output_open = true;
        next.agent_steps = steps.into();
        let update = AiTranscriptUpdate::between(
            Some(&previous),
            &next,
            &AiTranscriptRow::project(&previous),
            &AiTranscriptRow::project(&next),
        );
        assert_eq!(update.remeasure, vec![0..1]);
        assert!(update.splice.is_none() && !update.remeasure_all && !update.reset);
    }

    #[test]
    fn advancing_and_stopping_agent_refreshes_activity_in_retained_rows() {
        let root = TestConfigDir::new("nyaterm-ai-step-activity");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut previous =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let mut planning = linked_command_step(AiAgentStepStatus::Planning);
        planning.step.kind = crate::features::ai::presentation::AiAgentStepKind::Planning;
        planning.step.command_card_id = None;
        planning.step.source_message_id = None;
        previous.agent_steps = vec![planning.clone()].into();
        previous.running = true;
        assert!(previous.step_is_active(0));

        let mut next = previous.clone();
        planning.step.step_index = 1;
        next.agent_steps = vec![next.agent_steps[0].clone(), planning].into();
        assert!(!next.step_is_active(0));
        assert!(next.step_is_active(1));
        let update = AiTranscriptUpdate::between(
            Some(&previous),
            &next,
            &AiTranscriptRow::project(&previous),
            &AiTranscriptRow::project(&next),
        );
        assert!(update.splice.is_some());
        assert!(
            !AiTranscriptRow::project(&next)
                .iter()
                .any(|row| { matches!(row, AiTranscriptRow::AgentStep { step_index: 0, .. }) }),
            "earlier auxiliary activity is collapsed"
        );

        previous = next.clone();
        next.running = false;
        assert!(!next.step_is_active(1));
        let update = AiTranscriptUpdate::between(
            Some(&previous),
            &next,
            &AiTranscriptRow::project(&previous),
            &AiTranscriptRow::project(&next),
        );
        assert!(update.splice.is_some() && !update.reset);
        assert!(
            !AiTranscriptRow::project(&next)
                .iter()
                .any(|row| { matches!(row, AiTranscriptRow::AgentStep { .. }) })
        );
        next.agent_history_expanded = true;
        assert_eq!(
            AiTranscriptRow::project(&next)
                .iter()
                .filter(|row| { matches!(row, AiTranscriptRow::AgentStep { .. }) })
                .count(),
            2
        );
    }

    #[test]
    fn command_buttons_and_running_animation_match_execution_phase_in_a_narrow_panel() {
        use crate::features::ai::presentation::{AiAgentStepKind, AiCommandPhase};
        let root = TestConfigDir::new("nyaterm-ai-command-controls");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        for status in [
            AiAgentStepStatus::NeedsApproval,
            AiAgentStepStatus::Running,
            AiAgentStepStatus::Completed,
            AiAgentStepStatus::Failed,
            AiAgentStepStatus::Rejected,
            AiAgentStepStatus::Cancelled,
        ] {
            let mut step = linked_command_step(status);
            if status == AiAgentStepStatus::Completed {
                step.step.kind = AiAgentStepKind::Observation;
                step.step.exit_code = Some(0);
            }
            let phase = AiCommandPhase::from_step(&step.step);
            edit_panel_snapshot(&app, vcx, |snapshot| {
                let mut message = transcript_message(0);
                Arc::make_mut(&mut message).command_cards = vec![chat_card("agent-fixture")];
                snapshot.messages = vec![message].into();
                snapshot.command_cards = vec![chat_card("agent-fixture")].into();
                snapshot.agent_steps = vec![step].into();
            });
            assert_eq!(
                vcx.debug_bounds("ai-command-run-agent-fixture").is_some(),
                phase.offers_approval()
            );
            assert_eq!(
                vcx.debug_bounds("ai-command-reject-agent-fixture")
                    .is_some(),
                phase.offers_approval()
            );
            assert_eq!(
                vcx.debug_bounds("ai-command-insert-agent-fixture")
                    .is_some(),
                phase.offers_reuse()
            );
            assert_eq!(
                vcx.debug_bounds("ai-command-save-agent-fixture").is_some(),
                phase.offers_reuse()
            );
            assert!(vcx.debug_bounds("ai-command-copy-agent-fixture").is_some());
            assert_eq!(
                vcx.debug_bounds("ai-running-agent-fixture").is_some(),
                phase == AiCommandPhase::Running
            );
            let card = vcx.debug_bounds("ai-command-agent-fixture").unwrap();
            let viewport = vcx.debug_bounds("ai-transcript-viewport").unwrap();
            assert!(card.left() >= viewport.left() && card.right() <= viewport.right());
        }
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.agent_steps = Vec::new().into()
        });
        assert!(vcx.debug_bounds("ai-command-run-agent-fixture").is_none());
        assert!(
            vcx.debug_bounds("ai-command-reject-agent-fixture")
                .is_none()
        );
        assert!(
            vcx.debug_bounds("ai-command-insert-agent-fixture")
                .is_some()
        );
    }

    #[test]
    fn reasoning_disclosure_accepts_keyboard_and_persists_when_body_text_starts() {
        let root = TestConfigDir::new("nyaterm-ai-thought-keyboard");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let (job, id) = vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                let launch = app
                    .ai
                    .begin_chat_request("inspect".into(), AiMode::Ask, None);
                app.ai
                    .apply_chat_delta(launch.job_id, "", Some("A useful thought"));
                let id = app.ai.chat_streaming_assistant_id().unwrap().to_string();
                app.flush_ai_panel_snapshot(cx);
                (launch.job_id, id)
            })
        });
        compact_draw(&app, vcx);
        assert!(vcx.update(|_, cx| app.read(cx).ai.expanded_message_thoughts().is_empty()));
        // The first focusable element in this transcript is its reasoning disclosure.
        vcx.update(|window, cx| {
            window.blur(cx);
            window.focus_next(cx);
            window.draw(cx).clear(cx);
        });
        let enter = gpui::Keystroke::parse("enter").unwrap();
        vcx.simulate_event(gpui::KeyDownEvent {
            keystroke: enter.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(gpui::KeyUpEvent { keystroke: enter });
        compact_draw(&app, vcx);
        assert!(vcx.update(|_, cx| app.read(cx).ai.expanded_message_thoughts().contains(&id)));
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.ai.apply_chat_delta(job, "Answer", None);
                app.flush_ai_panel_snapshot(cx);
            })
        });
        compact_draw(&app, vcx);
        assert!(vcx.update(|_, cx| app.read(cx).ai.expanded_message_thoughts().contains(&id)));
        let space = gpui::Keystroke::parse("space").unwrap();
        vcx.simulate_event(gpui::KeyDownEvent {
            keystroke: space.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(gpui::KeyUpEvent { keystroke: space });
        compact_draw(&app, vcx);
        assert!(!vcx.update(|_, cx| app.read(cx).ai.expanded_message_thoughts().contains(&id)));
    }

    #[test]
    fn native_plan_stays_in_request_history_but_questions_remain_visible_and_focused() {
        use nyaterm_core::ai::harness::{
            AgentPlan, AgentPlanTask, AgentQuestion, AgentTaskStatus, AgentToolRegistry,
        };
        let root = TestConfigDir::new("nyaterm-ai-native-disclosure");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let user_id = vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.ai
                    .begin_chat_request("inspect".into(), AiMode::Agent, None);
                let user_id = app.ai.chat_snapshot_messages()[0].id.clone();
                app.ai.begin_native_run(
                    serde_json::from_value(serde_json::json!({
                        "mode": "agent", "action": "generate_command", "userInput": "inspect"
                    }))
                    .unwrap(),
                );
                app.ai
                    .update_native_plan(AgentPlan {
                        tasks: vec![AgentPlanTask {
                            id: "inspect".into(),
                            description: "Inspect resources".into(),
                            status: AgentTaskStatus::InProgress,
                            verification: None,
                        }],
                    })
                    .unwrap();
                app.flush_ai_panel_snapshot(cx);
                user_id
            })
        });
        compact_draw(&app, vcx);
        assert!(vcx.debug_bounds("ai-native-run").is_none());
        vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.ai.toggle_execution_group(user_id.clone());
                app.flush_ai_panel_snapshot(cx);
            })
        });
        compact_draw(&app, vcx);
        assert!(vcx.debug_bounds("ai-native-run").is_some());
        vcx.update(|_, cx| app.update(cx, |app, cx| {
            app.ai.toggle_execution_group(user_id.clone());
            let call = AgentToolRegistry::parse(&[], r#"{"action":"request_user_input","arguments":{"questions":[{"id":"service","question":"Which service?"}]}}"#).unwrap();
            app.ai.begin_native_call(call).unwrap();
            app.ai.wait_native_user(vec![AgentQuestion {
                id: "service".into(), question: "Which service?".into(), options: None,
            }]).unwrap();
            app.flush_ai_panel_snapshot(cx);
        }));
        compact_draw(&app, vcx);
        let question = vcx.debug_bounds("ai-native-run").unwrap();
        let viewport = vcx.debug_bounds("ai-transcript-viewport").unwrap();
        assert!(question.top() >= viewport.top());
        assert!(question.bottom() <= viewport.bottom());
        vcx.update(|window, cx| {
            let panel = app.read(cx).ai_panel.read(cx);
            let snapshot = panel.snapshot().unwrap();
            assert!(snapshot.expanded_execution_groups.is_empty());
            assert!(
                snapshot.native_answer_inputs[0]
                    .read(cx)
                    .component_focus_handle(cx)
                    .is_focused(window)
            );
            assert!(
                AiTranscriptRow::project(snapshot).iter().any(|row| {
                    matches!(row, AiTranscriptRow::NativeRun { history: false, .. })
                })
            );
        });
    }

    #[test]
    fn execution_disclosures_accept_keyboard_and_mouse_without_following_a_long_script_to_the_end()
    {
        let root = TestConfigDir::new("nyaterm-ai-execution-interaction");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let user_id = vcx.update(|_, cx| {
            app.update(cx, |app, cx| {
                let launch = app
                    .ai
                    .begin_chat_request("inspect".into(), AiMode::Agent, None);
                let user_id = app.ai.chat_snapshot_messages()[0].id.clone();
                let mut card = chat_card("agent-fixture");
                card.command = "echo fixture\n".repeat(100);
                app.ai
                    .finish_chat_job(
                        launch.job_id,
                        launch.session_id,
                        Ok(AiChatJobOutput {
                            native_call: None,
                            mode: AiMode::Agent,
                            text: "Inspect resources".into(),
                            reasoning: None,
                            command_cards: vec![card],
                            auto_execute_first: false,
                            approval_note: None,
                        }),
                    )
                    .unwrap();
                app.ai.upsert_agent_step(
                    0,
                    AiAgentStepStatus::Completed,
                    crate::features::ai::presentation::AiAgentStepKind::Observation,
                    "Observed",
                    "Resources are available",
                );
                app.flush_ai_panel_snapshot(cx);
                user_id
            })
        });
        compact_draw(&app, vcx);
        let scroll = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).transcript_scroll.clone());
        assert!(vcx.update(|_, cx| app.read(cx).ai.expanded_execution_groups().is_empty()));
        vcx.update(|window, cx| {
            window.blur(cx);
            window.focus_next(cx);
            window.draw(cx).clear(cx);
        });
        let enter = gpui::Keystroke::parse("enter").unwrap();
        vcx.simulate_event(gpui::KeyDownEvent {
            keystroke: enter.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(gpui::KeyUpEvent { keystroke: enter });
        compact_draw(&app, vcx);
        assert!(vcx.update(|_, cx| {
            app.read(cx)
                .ai
                .expanded_execution_groups()
                .contains(&user_id)
        }));
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        let command_toggle = vcx
            .debug_bounds("ai-command-toggle-agent-fixture-content")
            .unwrap();
        vcx.simulate_click(command_toggle.center(), gpui::Modifiers::default());
        compact_draw(&app, vcx);
        assert!(vcx.update(|_, cx| {
            app.read(cx)
                .ai
                .expanded_command_scripts()
                .contains("agent-fixture")
        }));
        assert!(!vcx.update(|_, cx| scroll.read(cx).is_following_tail()));
        let viewport = vcx.debug_bounds("ai-transcript-viewport").unwrap();
        let command_toggle = vcx
            .debug_bounds("ai-command-toggle-agent-fixture-content")
            .unwrap();
        assert!(command_toggle.top() >= viewport.top());
        assert!(command_toggle.bottom() <= viewport.bottom());
        assert!(
            vcx.debug_bounds("ai-command-body-agent-fixture")
                .unwrap()
                .size
                .height
                > viewport.size.height
        );
        vcx.simulate_click(command_toggle.center(), gpui::Modifiers::default());
        compact_draw(&app, vcx);
        assert!(!vcx.update(|_, cx| {
            app.read(cx)
                .ai
                .expanded_command_scripts()
                .contains("agent-fixture")
        }));
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        let selector = Box::leak(format!("ai-execution-toggle-{user_id}-content").into_boxed_str());
        let execution_toggle = vcx.debug_bounds(selector).unwrap();
        vcx.simulate_click(execution_toggle.center(), gpui::Modifiers::default());
        compact_draw(&app, vcx);
        assert!(!vcx.update(|_, cx| {
            app.read(cx)
                .ai
                .expanded_execution_groups()
                .contains(&user_id)
        }));
        assert!(
            vcx.debug_bounds("ai-command-toggle-agent-fixture")
                .is_none()
        );
    }

    #[test]
    fn historic_steps_without_a_command_card_fold_their_script_inside_the_request() {
        let root = TestConfigDir::new("nyaterm-ai-historic-command");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut messages: Vec<_> = (0..3).map(transcript_message).collect();
            for message in &mut messages[1..] {
                Arc::make_mut(message).role = AiMessageRole::Assistant;
            }
            let mut step = linked_command_step(AiAgentStepStatus::Completed);
            step.step.source_message_id = Some("message-1".into());
            step.step.command_card_id = None;
            snapshot.messages = messages.into();
            snapshot.agent_steps = vec![step].into();
            snapshot.expanded_execution_groups = vec!["message-0".into()].into();
        });
        assert!(vcx.debug_bounds("step-command-message-1-0").is_some());
        assert!(vcx.debug_bounds("ai-step-command-body-0").is_none());
        assert!(vcx.debug_bounds("ai-answer-message-2").is_some());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_command_scripts = vec!["step-command-message-1-0".into()].into();
        });
        assert!(vcx.debug_bounds("ai-step-command-body-0").is_some());
        assert!(vcx.debug_bounds("ai-answer-message-2").is_some());
    }

    #[test]
    fn request_disclosure_keeps_the_user_and_final_answer_visible_and_hides_commands() {
        let root = TestConfigDir::new("nyaterm-ai-request-disclosure");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut messages: Vec<_> = (0..4).map(transcript_message).collect();
            for message in &mut messages[1..] {
                Arc::make_mut(message).role = AiMessageRole::Assistant;
            }
            Arc::make_mut(&mut messages[1]).content =
                "I will inspect current resource usage.".into();
            let mut card = chat_card("agent-fixture");
            card.command = "echo resource\n".repeat(12);
            card.explanation = "Collect current resource metrics.".into();
            Arc::make_mut(&mut messages[2]).content = card.explanation.clone();
            Arc::make_mut(&mut messages[2]).command_cards = vec![card];
            Arc::make_mut(&mut messages[3]).content =
                "Resource usage is within normal limits.".into();
            snapshot.messages = messages.into();
            let mut step = linked_command_step(AiAgentStepStatus::Completed);
            step.step.source_message_id = Some("message-2".into());
            step.step.exit_code = Some(0);
            step.step.observation = Some("CPU 2%\nMemory 40%".into());
            snapshot.agent_steps = vec![step].into();
        });
        let collapsed =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let rows = AiTranscriptRow::project(&collapsed);
        assert_eq!(rows.len(), 3);
        assert!(matches!(
            &rows[0],
            AiTranscriptRow::Message { index: 0, .. }
        ));
        assert!(
            matches!(&rows[1], AiTranscriptRow::Execution { group } if group.id == "message-0")
        );
        assert!(matches!(
            &rows[2],
            AiTranscriptRow::FinalMessage { index: 3, .. }
        ));
        assert!(vcx.debug_bounds("ai-message-message-0").is_some());
        assert!(vcx.debug_bounds("ai-execution-toggle-message-0").is_some());
        assert!(vcx.debug_bounds("ai-answer-message-3").is_some());
        assert!(vcx.debug_bounds("ai-message-message-1").is_none());
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_execution_groups = vec!["message-0".into()].into();
        });
        assert!(vcx.debug_bounds("ai-message-message-1").is_some());
        assert!(
            vcx.debug_bounds("ai-command-toggle-agent-fixture")
                .is_some()
        );
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        assert!(vcx.debug_bounds("ai-answer-message-3").is_some());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_command_scripts = vec!["agent-fixture".into()].into();
        });
        assert!(
            vcx.debug_bounds("ai-command-body-agent-fixture")
                .unwrap()
                .size
                .height
                > px(62.)
        );
        assert!(
            vcx.debug_bounds("ai-command-output-agent-fixture")
                .is_some()
        );
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_execution_groups = Vec::new().into();
        });
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        assert!(vcx.debug_bounds("ai-answer-message-3").is_some());
        assert_eq!(AiTranscriptRow::project(&collapsed).len(), 3);
    }

    #[test]
    fn execution_groups_do_not_merge_requests_or_promote_tool_narration_to_final_answers() {
        use crate::features::ai::panel::execution::AiExecutionGroup;
        use crate::features::ai::presentation::{AiAgentStepKind, AiResponsePhase};
        let root = TestConfigDir::new("nyaterm-ai-request-boundaries");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut snapshot =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let mut messages: Vec<_> = (0..6).map(transcript_message).collect();
        for index in [1, 2, 4, 5] {
            Arc::make_mut(&mut messages[index]).role = AiMessageRole::Assistant;
        }
        Arc::make_mut(&mut messages[1]).command_cards = vec![chat_card("agent-first")];
        Arc::make_mut(&mut messages[4]).command_cards = vec![chat_card("agent-second")];
        snapshot.messages = messages.into();
        let groups = AiExecutionGroup::project(&snapshot);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "message-0");
        assert_eq!(groups[0].final_message, Some(2));
        assert_eq!(groups[1].id, "message-3");
        assert_eq!(groups[1].final_message, Some(5));
        snapshot.expanded_execution_groups = vec!["message-0".into()].into();
        let rows = AiTranscriptRow::project(&snapshot);
        assert!(
            rows.iter()
                .any(|row| matches!(row, AiTranscriptRow::ActivityMessage { index: 1, .. }))
        );
        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, AiTranscriptRow::ActivityMessage { index: 4, .. }))
        );
        let mut step = linked_command_step(AiAgentStepStatus::Running);
        step.step.kind = AiAgentStepKind::ToolProgress;
        step.step.command_card_id = None;
        step.step.source_message_id = Some("message-5".into());
        snapshot.agent_steps = vec![step].into();
        snapshot.running = true;
        snapshot.streaming_assistant_id = Some("message-5".into());
        snapshot.response_phase = AiResponsePhase::ToolArguments;
        let groups = AiExecutionGroup::project(&snapshot);
        assert!(!groups[0].is_live(&snapshot));
        assert!(groups[1].is_live(&snapshot));
        assert_eq!(
            groups[1].final_message, None,
            "tool narration stays in the process"
        );
    }

    #[test]
    fn tool_preparation_belongs_to_the_streaming_message_and_remeasures_it() {
        use crate::features::ai::presentation::{AiAgentStepKind, AiResponsePhase};
        let root = TestConfigDir::new("nyaterm-ai-inline-progress");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        let mut previous =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let mut message = transcript_message(0);
        Arc::make_mut(&mut message).role = AiMessageRole::Assistant;
        Arc::make_mut(&mut message).content.clear();
        previous.messages = vec![message].into();
        previous.running = true;
        previous.streaming_assistant_id = Some("message-0".into());
        previous.response_phase = AiResponsePhase::ToolArguments;
        let mut next = previous.clone();
        let mut step = linked_command_step(AiAgentStepStatus::Tool);
        step.step.kind = AiAgentStepKind::ToolProgress;
        step.step.command_card_id = None;
        step.step.source_message_id = None;
        step.step.command = None;
        step.step.title = "Tool tool".into();
        step.step.detail = "Streaming arguments (+2 chars)".into();
        next.agent_steps = vec![step].into();
        let rows = AiTranscriptRow::project(&next);
        assert_eq!(rows.len(), 1, "preparation becomes one execution header");
        assert!(matches!(&rows[0], AiTranscriptRow::Execution { .. }));
        let update = AiTranscriptUpdate::between(
            Some(&previous),
            &next,
            &AiTranscriptRow::project(&previous),
            &rows,
        );
        assert!(update.splice.is_some());
        next.expanded_execution_groups = vec!["message-0".into()].into();
        edit_panel_snapshot(&app, vcx, |snapshot| *snapshot = next.clone());
        assert!(vcx.debug_bounds("ai-thinking-message-0").is_none());
        assert!(vcx.debug_bounds("ai-agent-step-0").is_some());
        assert!(vcx.debug_bounds("ai-step-running-0").is_some());
    }

    #[test]
    fn command_expansion_preserves_results_and_approval_shows_the_complete_script() {
        let root = TestConfigDir::new("nyaterm-ai-script-expansion");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            let mut message = transcript_message(0);
            Arc::make_mut(&mut message).role = AiMessageRole::Assistant;
            let mut card = chat_card("agent-fixture");
            card.title = "Agent Command".into();
            card.explanation = "Inspect current resource usage".into();
            card.command = "echo resource\n".repeat(12);
            Arc::make_mut(&mut message).content = card.explanation.clone();
            Arc::make_mut(&mut message).command_cards = vec![card];
            snapshot.messages = vec![message].into();
            let mut step = linked_command_step(AiAgentStepStatus::Completed);
            step.step.exit_code = Some(0);
            step.step.observation = Some("CPU 2%\nMemory 40%".into());
            step.step.thought = Some("Inspect current resource usage".into());
            snapshot.agent_steps = vec![step].into();
            snapshot.expanded_execution_groups = vec!["message-0".into()].into();
        });
        assert!(vcx.debug_bounds("ai-answer-message-0").is_none());
        assert!(
            vcx.debug_bounds("ai-command-thought-agent-fixture")
                .is_none()
        );
        assert!(vcx.debug_bounds("ai-command-body-agent-fixture").is_none());
        assert!(
            vcx.debug_bounds("ai-command-toggle-agent-fixture")
                .is_some()
        );
        let previous =
            vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_command_scripts = vec!["agent-fixture".into()].into();
        });
        assert!(
            vcx.debug_bounds("ai-command-body-agent-fixture")
                .unwrap()
                .size
                .height
                > px(62.)
        );
        let next = vcx.update(|_, cx| app.read(cx).ai_panel.read(cx).snapshot().unwrap().clone());
        let update = AiTranscriptUpdate::between(
            Some(&previous),
            &next,
            &AiTranscriptRow::project(&previous),
            &AiTranscriptRow::project(&next),
        );
        assert_eq!(update.remeasure, vec![1..2]);
        edit_panel_snapshot(&app, vcx, |snapshot| {
            snapshot.expanded_command_scripts = Vec::new().into();
            snapshot.expanded_execution_groups = Vec::new().into();
            let mut steps = snapshot.agent_steps.to_vec();
            steps[0].step.status = AiAgentStepStatus::NeedsApproval;
            steps[0].step.observation = None;
            snapshot.agent_steps = steps.into();
        });
        assert!(
            vcx.debug_bounds("ai-command-body-agent-fixture")
                .unwrap()
                .size
                .height
                > px(62.)
        );
        assert!(
            vcx.debug_bounds("ai-command-script-agent-fixture")
                .is_none()
        );
        assert!(vcx.debug_bounds("ai-command-run-agent-fixture").is_some());
    }

    #[test]
    fn long_commands_and_collapsed_output_fit_light_and_dark_panel_layouts() {
        let root = TestConfigDir::new("nyaterm-ai-command-themes");
        let mut cx = TestAppContext::single();
        let (app, vcx) = compact_host(&mut cx, root.path());
        for theme in ["github-dark", "solarized-light"] {
            let palette = nyaterm_ui::theme_palette(theme);
            vcx.update(|_, cx| {
                nyaterm_ui::apply_component_theme(palette, gpui::font("Arial"), px(12.), cx)
            });
            edit_panel_snapshot(&app, vcx, |snapshot| {
                let mut card = chat_card("agent-fixture");
                card.command = "printf '%s' ".repeat(32);
                let mut message = transcript_message(0);
                Arc::make_mut(&mut message).command_cards = vec![card];
                snapshot.messages = vec![message].into();
                snapshot.chrome.palette = palette;
                let mut step = linked_command_step(AiAgentStepStatus::Completed);
                step.step.kind = crate::features::ai::presentation::AiAgentStepKind::Observation;
                step.step.exit_code = Some(0);
                step.step.observation = Some("resource output\n".repeat(400));
                snapshot.agent_steps = vec![step].into();
            });
            let command = vcx.debug_bounds("ai-command-body-agent-fixture").unwrap();
            let viewport = vcx.debug_bounds("ai-transcript-viewport").unwrap();
            assert!(command.left() >= viewport.left() && command.right() <= viewport.right());
            assert!(
                command.size.height <= px(62.),
                "long command preview must stay bounded"
            );
            assert!(
                vcx.debug_bounds("ai-command-script-agent-fixture")
                    .is_some()
            );
            let result = vcx.debug_bounds("ai-command-result-agent-fixture").unwrap();
            assert!(result.top() < command.top(), "results precede the command");
            assert!(
                vcx.debug_bounds("ai-command-output-agent-fixture")
                    .is_some()
            );
            for selector in [
                "ai-command-output-agent-fixture-content",
                "ai-command-details-agent-fixture-content",
            ] {
                let content = vcx.debug_bounds(selector).unwrap();
                let card = vcx.debug_bounds("ai-command-agent-fixture").unwrap();
                assert_eq!(content.size.height, px(20.));
                assert!(
                    content.left() - card.left() < px(16.),
                    "disclosures must align with the card's left padding"
                );
                assert!(
                    content.size.width < card.size.width * 0.75,
                    "disclosure buttons must keep their content width"
                );
            }
            assert!(vcx.debug_bounds("ai-command-copy-agent-fixture").is_some());
        }
    }
}
