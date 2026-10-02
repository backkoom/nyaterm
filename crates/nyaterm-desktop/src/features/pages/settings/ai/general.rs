use super::super::{settings_form_row, settings_switch};
use super::{ai_card, ai_field};
use crate::features::pages::settings::panel::SettingsPanel;
use gpui::{Context, IntoElement, div, prelude::*, px, rgb};
use nyaterm_ui::NyaSelectOption;
use rust_i18n::t;

impl SettingsPanel {
    pub(super) fn ai_general_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.theme_palette();
        let settings = self.ai.settings_config().clone();
        let switches = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(settings_form_row(
                palette,
                t!("ai.enabled"),
                None,
                settings_switch(
                    palette,
                    "ai-enabled",
                    settings.enabled,
                    cx.listener(|panel, _, _, cx| panel.toggle_ai_enabled(cx)),
                ),
            ))
            .child(settings_form_row(
                palette,
                t!("ai.redaction"),
                None,
                settings_switch(
                    palette,
                    "ai-redaction-toggle",
                    settings.redaction_enabled,
                    cx.listener(|panel, _, _, cx| panel.toggle_ai_redaction(cx)),
                ),
            ))
            .child(settings_form_row(
                palette,
                t!("ai.allowSave"),
                None,
                settings_switch(
                    palette,
                    "ai-save-command-toggle",
                    settings.allow_save_command,
                    cx.listener(|panel, _, _, cx| panel.toggle_ai_allow_save_command(cx)),
                ),
            ))
            .child(settings_form_row(
                palette,
                t!("ai.recordHistory"),
                None,
                settings_switch(
                    palette,
                    "ai-history-toggle",
                    settings.record_history,
                    cx.listener(|panel, _, _, cx| panel.toggle_ai_record_history(cx)),
                ),
            ));
        let fields = div()
            .grid()
            .grid_cols(if self.viewport_width >= 1024. { 2 } else { 1 })
            .gap_4()
            .child(ai_field(
                palette,
                t!("ai.contextLineLimit"),
                None,
                self.existing_number_input_box("ai.number.context-line-limit"),
            ))
            .child(ai_field(
                palette,
                t!("ai.timeoutMs"),
                None,
                self.existing_number_input_box("ai.number.timeout-ms"),
            ));
        let general = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(switches)
            .child(ai_field(
                palette,
                t!("ai.requestUserAgent"),
                Some(t!("ai.requestUserAgentDesc").into()),
                self.existing_text_input_box("ai.input.request-user-agent", false),
            ))
            .child(fields);
        let risk = match settings.agent_smart_auto_execute_max_risk {
            nyaterm_core::RiskLevel::Low => "low",
            nyaterm_core::RiskLevel::Medium => "medium",
            nyaterm_core::RiskLevel::High => "high",
            nyaterm_core::RiskLevel::Critical => "critical",
        };
        let risk = self.form_select_control(
            "ai-smart-risk",
            [
                ("low", "ai.riskLow"),
                ("medium", "ai.riskMedium"),
                ("high", "ai.riskHigh"),
                ("critical", "ai.riskCritical"),
            ]
            .into_iter()
            .map(|(value, label)| NyaSelectOption::new(value, t!(label)))
            .collect(),
            Some(risk.into()),
            false,
            cx,
        );
        let params = div()
            .grid()
            .grid_cols(if self.viewport_width >= 1024. { 2 } else { 1 })
            .gap_4()
            .child(ai_field(
                palette,
                t!("ai.agentMaxSteps"),
                None,
                self.existing_number_input_box("ai.number.agent-steps"),
            ))
            .child(ai_field(
                palette,
                t!("ai.agentStepTimeout"),
                None,
                self.existing_number_input_box("ai.number.agent-step-timeout-ms"),
            ))
            .child(ai_field(
                palette,
                t!("ai.terminalOutputLines"),
                None,
                self.existing_number_input_box("ai.number.terminal-output-lines"),
            ));
        let agent = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(ai_field(
                palette,
                t!("ai.smartAutoExecuteMaxRisk"),
                Some(t!("ai.smartAutoExecuteMaxRiskDesc").into()),
                risk,
            ))
            .child(params)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("ai.agentMaxStepsDesc")),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(palette.text_muted))
                    .child(t!("ai.terminalOutputLinesDesc")),
            );
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(ai_card(
                palette,
                div().child(t!("ai.general")),
                div(),
                general,
            ))
            .child(ai_card(
                palette,
                div().child(t!("ai.agentSettings")),
                div(),
                agent,
            ))
    }
}
