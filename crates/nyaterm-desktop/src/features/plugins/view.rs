use crate::features::NyaTermApp;
use crate::features::plugins::process::PluginProcess;
use gpui::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled,
    Subscription, Task, WeakEntity, Window, div, px, rgb,
};
use nyaterm_core::plugins::invocation::{ActionInput, ActionResult, parse_parameter};
use nyaterm_core::plugins::manifest::{ParameterKind, ResultKind};
use nyaterm_core::plugins::{ErrorCode, MAX_TEXT_BYTES, PluginError, PluginStatus};
use nyaterm_plugin_host::manager::{Contribution, InvocationLease, PluginSource};
use nyaterm_plugin_host::service::{OperationReply, PluginOperation};
use nyaterm_ui::{NyaButton, NyaButtonVariant, NyaInput, NyaInputState, NyaScrollable};
use rust_i18n::t;

struct ParameterInput {
    id: String,
    name: String,
    kind: ParameterKind,
    required: bool,
    input: Entity<NyaInputState>,
}
struct Preview {
    plugin_id: String,
    contribution_id: String,
    revision: u64,
    kind: ResultKind,
    title: String,
    lease: InvocationLease,
}

pub(in crate::features) struct PluginPanel {
    app: WeakEntity<NyaTermApp>,
    pub(super) process: Entity<PluginProcess>,
    source: Entity<NyaInputState>,
    text: Entity<NyaInputState>,
    output: Entity<NyaInputState>,
    parameters: Vec<ParameterInput>,
    selected: Option<Contribution>,
    preview: Option<Preview>,
    status: String,
    pending: Option<Task<()>>,
    generation: u64,
    busy: bool,
    _subscription: Subscription,
}

impl PluginPanel {
    pub fn new(
        app: WeakEntity<NyaTermApp>,
        process: Entity<PluginProcess>,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&process, |this, _, cx| {
            let snapshot = &this.process.read(cx).snapshot;
            let valid = this.selected.as_ref().is_none_or(|selected| {
                snapshot
                    .contributions
                    .iter()
                    .any(|c| c.id == selected.id && c.revision == selected.revision)
            });
            if !valid {
                this.pending.take();
                this.busy = false;
                this.generation += 1;
                this.selected = None;
                this.preview = None;
                this.parameters.clear();
                this.output.update(cx, |state, cx| state.clear(cx));
                this.status = t!("plugins.actionChanged").to_string();
            }
            cx.notify();
        });
        Self {
            app,
            process,
            source: cx.new(|cx| {
                NyaInputState::new(cx, "")
                    .placeholder(t!("plugins.source"))
                    .max_chars(Some(32768))
            }),
            text: cx.new(|cx| {
                NyaInputState::new(cx, "")
                    .multi_line(Some(6))
                    .max_chars(Some(MAX_TEXT_BYTES))
            }),
            output: cx.new(|cx| {
                NyaInputState::new(cx, "")
                    .multi_line(Some(6))
                    .max_chars(Some(MAX_TEXT_BYTES))
            }),
            parameters: Vec::new(),
            selected: None,
            preview: None,
            status: String::new(),
            pending: None,
            generation: 0,
            busy: false,
            _subscription: subscription,
        }
    }

    fn select(&mut self, contribution: Contribution, cx: &mut Context<Self>) {
        self.pending.take();
        self.generation += 1;
        self.busy = false;
        self.preview = None;
        self.output.update(cx, |state, cx| state.clear(cx));
        self.parameters = contribution
            .action
            .parameters
            .iter()
            .map(|parameter| ParameterInput {
                id: parameter.id.clone(),
                name: parameter.name.clone(),
                kind: parameter.kind,
                required: parameter.required,
                input: cx.new(|cx| {
                    NyaInputState::new(cx, parameter.default.clone().unwrap_or_default())
                        .max_chars(Some(MAX_TEXT_BYTES))
                }),
            })
            .collect();
        self.selected = Some(contribution);
        self.status.clear();
        cx.notify();
    }

    fn source_path(&self, cx: &gpui::App) -> std::path::PathBuf {
        std::path::PathBuf::from(self.source.read(cx).value(cx).trim())
    }

    fn operation(&mut self, operation: PluginOperation, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(service) = self.process.read(cx).service.clone() else {
            self.status = t!("plugins.unavailable").to_string();
            cx.notify();
            return;
        };
        let task = match service.submit(operation) {
            Ok(task) => task,
            Err(error) => {
                self.status = error.message;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.generation += 1;
        let generation = self.generation;
        self.status = t!("plugins.working").to_string();
        self.pending = Some(cx.spawn(async move |this, cx| {
            let result = match task.await {
                Ok(Ok(OperationReply::Changed)) => Ok(None),
                Ok(Ok(OperationReply::Invocation(invocation))) => {
                    let source = (
                        invocation.plugin_id.clone(),
                        invocation.contribution_id.clone(),
                        invocation.revision,
                        invocation.lease(),
                    );
                    invocation
                        .result()
                        .await
                        .map(|result| Some((source, result)))
                }
                Ok(Err(error)) => Err(error),
                Err(_) => Err(PluginError::new(
                    ErrorCode::Shutdown,
                    "Plugin operation was interrupted",
                )),
            };
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                match result {
                    Ok(Some(((plugin_id, contribution_id, revision, lease), result))) => {
                        let current = service.snapshot();
                        if !lease.is_current()
                            || !current
                                .contributions
                                .iter()
                                .any(|c| c.id == contribution_id && c.revision == revision)
                        {
                            this.status = t!("plugins.actionChanged").to_string();
                            cx.notify();
                            return;
                        }
                        let (kind, title, output) = match result {
                            ActionResult::Command { title, command } => {
                                (ResultKind::Command, title, command)
                            }
                            ActionResult::Text(text) => {
                                (ResultKind::Text, t!("plugins.textResult").to_string(), text)
                            }
                        };
                        this.output
                            .update(cx, |state, cx| state.set_content(&output, cx));
                        this.preview = Some(Preview {
                            plugin_id,
                            contribution_id,
                            revision,
                            kind,
                            title,
                            lease,
                        });
                        this.status = t!("plugins.ready").to_string();
                    }
                    Ok(None) => this.status = t!("plugins.done").to_string(),
                    Err(error) => {
                        this.preview = None;
                        this.status = error.message;
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn invoke(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let mut input = ActionInput::default();
        for parameter in &self.parameters {
            let value = parameter.input.read(cx).value(cx);
            if value.is_empty() && !parameter.required {
                continue;
            }
            match parse_parameter(parameter.kind, &value) {
                Ok(value) => {
                    input.parameters.insert(parameter.id.clone(), value);
                }
                Err(error) => {
                    self.status = error.message;
                    cx.notify();
                    return;
                }
            }
        }
        if selected.action.result == ResultKind::Text {
            input.text = Some(self.text.read(cx).value(cx));
        }
        self.preview = None;
        self.operation(
            PluginOperation::Invoke {
                contribution_id: selected.id,
                input,
            },
            cx,
        );
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.pending.take();
        self.generation += 1;
        self.busy = false;
        self.status = t!("plugins.cancelled").to_string();
        cx.notify();
    }

    fn fill(&mut self, replace: bool, cx: &mut Context<Self>) {
        let Some(preview) = &self.preview else {
            return;
        };
        let Some(service) = &self.process.read(cx).service else {
            return;
        };
        if !preview.lease.is_current()
            || preview.kind != ResultKind::Command
            || !service
                .snapshot()
                .contributions
                .iter()
                .any(|c| c.id == preview.contribution_id && c.revision == preview.revision)
        {
            self.status = t!("plugins.actionChanged").to_string();
            cx.notify();
            return;
        }
        let output = self.output.read(cx).value(cx);
        self.status = match self
            .app
            .update(cx, |app, cx| app.fill_plugin_draft(&output, replace, cx))
        {
            Ok(Ok(())) => t!("plugins.draftFilled").to_string(),
            Ok(Err(error)) => error.message,
            Err(_) => t!("plugins.unavailable").to_string(),
        };
        cx.notify();
    }

    fn browse(&mut self, directories: bool, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some(t!("plugins.chooseSource").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.first()
            {
                let text = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |this, cx| {
                    this.source
                        .update(cx, |state, cx| state.set_content(&text, cx))
                });
            }
        })
        .detach();
    }
}

fn status_label(status: PluginStatus) -> String {
    t!(match status {
        PluginStatus::Installed => "plugins.installed",
        PluginStatus::Enabled => "plugins.enabled",
        PluginStatus::Running => "plugins.running",
        PluginStatus::Incompatible => "plugins.incompatible",
        PluginStatus::LoadFailed => "plugins.loadFailed",
        PluginStatus::Faulted => "plugins.faulted",
    })
    .to_string()
}

impl Render for PluginPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self
            .app
            .upgrade()
            .map(|app| app.read(cx).theme_palette())
            .unwrap_or_else(|| crate::theme::theme_palette("dark"));
        let snapshot = self.process.read(cx).snapshot.clone();
        let mut content = div()
            .id("plugins-scroll")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p_2()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().child(t!("plugins.boundary")))
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .h(px(32.))
                    .child(NyaInput::new(&self.source)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        NyaButton::new("plugins-directory", t!("plugins.directory"))
                            .on_click(cx.listener(|this, _, _, cx| this.browse(true, cx))),
                    )
                    .child(
                        NyaButton::new("plugins-archive", t!("plugins.archive"))
                            .on_click(cx.listener(|this, _, _, cx| this.browse(false, cx))),
                    )
                    .child(
                        NyaButton::new("plugins-install", t!("plugins.install"))
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let source = this.source_path(cx);
                                this.operation(
                                    PluginOperation::Install {
                                        source,
                                        development: false,
                                    },
                                    cx,
                                );
                            })),
                    )
                    .child(
                        NyaButton::new("plugins-development", t!("plugins.development"))
                            .full_width()
                            .tooltip(t!("plugins.development"))
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let source = this.source_path(cx);
                                this.operation(
                                    PluginOperation::Install {
                                        source,
                                        development: true,
                                    },
                                    cx,
                                );
                            })),
                    ),
            )
            .child(
                div()
                    .text_color(rgb(palette.text_muted))
                    .child(t!("plugins.retainedData")),
            )
            .child(div().child(self.status.clone()));
        if let Some(error) = snapshot.startup_error {
            content = content.child(div().text_color(rgb(palette.danger)).child(error.message));
        }
        if snapshot.plugins.is_empty() {
            content = content.child(div().child(t!("plugins.empty")));
        }
        for plugin in snapshot.plugins {
            let name = plugin
                .manifest
                .as_ref()
                .map_or(plugin.id.clone(), |m| format!("{} · {}", m.name, m.version));
            let mut card = div()
                .w_full()
                .min_w_0()
                .flex_none()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(palette.border))
                .flex()
                .flex_col()
                .gap_2()
                .child(div().child(format!("{name} · {}", status_label(plugin.status))))
                .child(div().text_color(rgb(palette.text_muted)).child(format!(
                    "{} · {}",
                    plugin.id,
                    match &plugin.source {
                        PluginSource::Managed => t!("plugins.managed").to_string(),
                        PluginSource::Development(path) =>
                            format!("{}: {}", t!("plugins.development"), path.display()),
                    }
                )));
            if let Some(manifest) = &plugin.manifest {
                card = card.child(div().child(manifest.description.clone())).child(
                    div().text_color(rgb(palette.text_muted)).child(format!(
                        "{} · API {} · schema {} · NyaTerm {}",
                        manifest.authors.join(", "),
                        manifest.api_version,
                        manifest.schema_version,
                        manifest.host_version
                    )),
                );
            }
            if let Some(error) = &plugin.error {
                card = card.child(
                    div()
                        .text_color(rgb(palette.danger))
                        .child(error.message.clone()),
                );
            }
            let toggle_id = plugin.id.clone();
            let reload_id = plugin.id.clone();
            let update_id = plugin.id.clone();
            let uninstall_id = plugin.id.clone();
            let enabled = plugin.enabled;
            card = card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        NyaButton::new(
                            format!("plugins-toggle-{}", plugin.id),
                            if enabled {
                                t!("plugins.disable")
                            } else {
                                t!("plugins.enable")
                            },
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.operation(
                                PluginOperation::Enable {
                                    id: toggle_id.clone(),
                                    enabled: !enabled,
                                },
                                cx,
                            )
                        })),
                    )
                    .child(
                        NyaButton::new(
                            format!("plugins-reload-{}", plugin.id),
                            t!("plugins.reload"),
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.operation(
                                PluginOperation::Reload {
                                    id: reload_id.clone(),
                                },
                                cx,
                            )
                        })),
                    )
                    .child(
                        NyaButton::new(
                            format!("plugins-update-{}", plugin.id),
                            t!("plugins.update"),
                        )
                        .full_width()
                        .tooltip(t!("plugins.update"))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let source = this.source_path(cx);
                            this.operation(
                                PluginOperation::Update {
                                    id: update_id.clone(),
                                    source,
                                },
                                cx,
                            );
                        })),
                    )
                    .child(
                        NyaButton::new(
                            format!("plugins-uninstall-{}", plugin.id),
                            t!("plugins.uninstall"),
                        )
                        .full_width()
                        .tooltip(t!("plugins.uninstall"))
                        .variant(NyaButtonVariant::Danger)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.operation(
                                PluginOperation::Uninstall {
                                    id: uninstall_id.clone(),
                                },
                                cx,
                            )
                        })),
                    ),
            );
            let mut actions = div().flex().flex_wrap().gap_2();
            for contribution in snapshot
                .contributions
                .iter()
                .filter(|c| c.plugin_id == plugin.id)
            {
                let selected = contribution.clone();
                actions = actions.child(
                    NyaButton::new(
                        format!("plugins-action-{}", contribution.id),
                        contribution.action.name.clone(),
                    )
                    .full_width()
                    .tooltip(contribution.action.name.clone())
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.select(selected.clone(), cx))),
                );
            }
            content = content.child(card.child(actions));
        }
        if let Some(selected) = &self.selected {
            let mut form = div()
                .w_full()
                .min_w_0()
                .flex_none()
                .p_3()
                .rounded_lg()
                .border_1()
                .border_color(rgb(palette.border))
                .flex()
                .flex_col()
                .gap_2()
                .child(div().child(format!("{} · {}", selected.action.name, selected.id)))
                .child(div().child(selected.action.description.clone()));
            for parameter in &self.parameters {
                form = form.child(div().child(parameter.name.clone())).child(
                    div()
                        .w_full()
                        .flex_none()
                        .h(px(32.))
                        .child(NyaInput::new(&parameter.input)),
                );
            }
            if selected.action.result == ResultKind::Text {
                form = form.child(div().child(t!("plugins.explicitText"))).child(
                    div()
                        .w_full()
                        .flex_none()
                        .h(px(120.))
                        .child(NyaInput::new(&self.text)),
                );
            }
            form = form.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        NyaButton::new("plugins-invoke", t!("plugins.invoke"))
                            .full_width()
                            .tooltip(t!("plugins.invoke"))
                            .variant(NyaButtonVariant::Primary)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.invoke(cx))),
                    )
                    .child(
                        NyaButton::new("plugins-cancel", t!("plugins.cancel"))
                            .disabled(!self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    ),
            );
            content = content.child(form);
        }
        if let Some(preview) = &self.preview {
            let mut result =
                div()
                    .w_full()
                    .min_w_0()
                    .flex_none()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(palette.border))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().child(format!("{} · {}", preview.title, preview.plugin_id)))
                    .child(
                        div()
                            .w_full()
                            .flex_none()
                            .h(px(150.))
                            .child(NyaInput::new(&self.output)),
                    )
                    .child(NyaButton::new("plugins-copy", t!("plugins.copy")).on_click(
                        cx.listener(|this, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                this.output.read(cx).value(cx),
                            ));
                        }),
                    ));
            if preview.kind == ResultKind::Command {
                result = result.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            NyaButton::new("plugins-fill", t!("plugins.fill"))
                                .full_width()
                                .tooltip(t!("plugins.fill"))
                                .on_click(cx.listener(|this, _, _, cx| this.fill(false, cx))),
                        )
                        .child(
                            NyaButton::new("plugins-replace", t!("plugins.replace"))
                                .full_width()
                                .tooltip(t!("plugins.replace"))
                                .on_click(cx.listener(|this, _, _, cx| this.fill(true, cx))),
                        ),
                );
            } else {
                result = result.child(
                    NyaButton::new("plugins-reuse", t!("plugins.reuse"))
                        .full_width()
                        .tooltip(t!("plugins.reuse"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let value = this.output.read(cx).value(cx);
                            this.text
                                .update(cx, |state, cx| state.set_content(&value, cx));
                        })),
                );
            }
            content = content.child(result);
        }
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(rgb(palette.surface))
            .text_color(rgb(palette.text))
            .text_sm()
            .whitespace_normal()
            .child(content.overflow_y_scrollbar())
    }
}

#[cfg(test)]
mod tests {
    use crate::features::plugins::process::PluginProcess;
    use crate::features::plugins::view::PluginPanel;
    use crate::features::test_support::app_with_visible_local_session;
    use crate::models::{ActivityBarZone, NavItem};
    use futures::executor::block_on;
    use gpui::{AppContext, TestAppContext};
    use nyaterm_core::runtime::{AppRuntime, RuntimeMode};
    use nyaterm_core::test_support::TestTempDir;
    use nyaterm_plugin_host::service::PluginOperation;
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn manager_renders_and_real_guest_results_require_explicit_fill_and_valid_revision() {
        let root = TestTempDir::new("nyaterm-plugin-manager-view");
        let app_root = TestTempDir::new("nyaterm-plugin-manager-app");
        let mut cx = TestAppContext::single();
        let app = app_with_visible_local_session(&mut cx, app_root.path(), "draft-session");
        cx.update_entity(&app, |app, cx| {
            app.sync_component_theme(cx);
            app.apply_send_command_draft("existing user draft".into(), cx);
        });
        let runtime = AppRuntime::from_parts_for_test(
            RuntimeMode::Portable,
            root.path().into(),
            root.join("config"),
            root.join("logs"),
            root.join("cache"),
            None,
        );
        let process = cx.new(|cx| PluginProcess::new(runtime, cx));
        let service = cx.read(|cx| process.read(cx).service().unwrap());
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        std::fs::copy(
            crate_root.join("../../examples/plugins/diagnostic-command/plugin.toml"),
            source.join("plugin.toml"),
        )
        .unwrap();
        std::fs::copy(
            crate_root.join("../nyaterm-plugin-host/tests/fixtures/diagnostic-command.wasm"),
            source.join("plugin.wasm"),
        )
        .unwrap();
        block_on(
            service
                .submit(PluginOperation::Install {
                    source,
                    development: false,
                })
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        let panel_app = app.downgrade();
        let panel_process = process.clone();
        let (panel, vcx) =
            cx.add_window_view(move |_, cx| PluginPanel::new(panel_app, panel_process, cx));
        vcx.update_entity(&app, |app, cx| {
            app.plugins.panel = panel.clone();
            app.open_panel(NavItem::Plugins, cx);
        });
        vcx.run_until_parked();
        let contribution = service.snapshot().contributions[0].clone();
        vcx.update_entity(&panel, |panel, cx| {
            panel.select(contribution, cx);
            panel.invoke(cx);
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            vcx.run_until_parked();
            if vcx.read(|cx| !panel.read(cx).busy) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "manager invocation did not complete"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..3 {
            vcx.update(|window, cx| {
                panel.update(cx, |_, cx| cx.notify());
                let _ = window.draw(cx);
            });
            vcx.run_until_parked();
        }
        let generation = vcx.read(|cx| panel.read(cx).generation);
        vcx.update_entity(&app, |app, cx| {
            app.hide_activity_entry("plugins".into(), cx);
            app.show_activity_entry("plugins".into(), cx);
            app.move_activity_entry("plugins".into(), ActivityBarZone::RightBottom, None, cx);
            app.open_panel(NavItem::Plugins, cx);
            assert_eq!(app.plugins.panel.entity_id(), panel.entity_id());
        });
        vcx.update_entity(&panel, |panel, cx| {
            assert_eq!(panel.generation, generation);
            assert!(panel.selected.is_some());
            assert!(panel.preview.is_some(), "{}", panel.status);
            assert_eq!(
                panel.output.read(cx).value(cx),
                "nslookup example.org\nping example.org"
            );
            let app = panel.app.upgrade().unwrap();
            assert_eq!(
                app.read(cx).send_command.presentation().draft,
                "existing user draft"
            );
            panel.fill(false, cx);
            assert_eq!(
                app.read(cx).send_command.presentation().draft,
                "existing user draft\nnslookup example.org\nping example.org"
            );
            assert!(!app.read(cx).send_command.presentation().sending);
        });
        block_on(
            service
                .submit(PluginOperation::Enable {
                    id: "diagnostic-command".into(),
                    enabled: false,
                })
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        // Even before the catalog event is rendered, a revoked preview cannot fill.
        vcx.update_entity(&panel, |panel, cx| {
            let before = panel
                .app
                .upgrade()
                .unwrap()
                .read(cx)
                .send_command
                .presentation()
                .draft;
            panel.fill(true, cx);
            assert_eq!(
                panel
                    .app
                    .upgrade()
                    .unwrap()
                    .read(cx)
                    .send_command
                    .presentation()
                    .draft,
                before
            );
        });
        vcx.run_until_parked();
        assert!(vcx.read(|cx| panel.read(cx).preview.is_none()));
        service.shutdown();
    }
}
