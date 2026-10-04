use crate::features::NyaTermApp;
use crate::features::plugins::process::PluginProcess;
use crate::features::plugins::view::PluginPanel;
use crate::features::view_widgets::{ChildWindowSpec, child_window_options};
use gpui::{App, AppContext, Context, Entity, Window};
use nyaterm_ui::{ChildWindowSlot, NyaWindowHandle, activate_child_window, nya_root};
use rust_i18n::t;

pub(in crate::features) struct PluginFeatureState {
    pub process: Entity<PluginProcess>,
    pub window: ChildWindowSlot,
}
impl PluginFeatureState {
    pub fn new(process: Entity<PluginProcess>) -> Self {
        Self {
            process,
            window: ChildWindowSlot::default(),
        }
    }
}

impl NyaTermApp {
    pub(in crate::features) fn open_plugin_manager(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = self.plugins.window.handle() {
            activate_child_window(
                &cx.entity(),
                handle,
                |app| Some(&mut app.plugins.window),
                cx,
            );
            return;
        }
        if !self.plugins.window.begin_open() {
            return;
        }
        let app = cx.entity();
        let parent = window
            .window_handle()
            .downcast::<nyaterm_ui::NyaRoot>()
            .or(self.shell.main_window());
        cx.defer(move |cx| open(app, parent, cx));
    }
}

fn open(app: Entity<NyaTermApp>, parent: Option<NyaWindowHandle>, cx: &mut App) {
    let spec =
        ChildWindowSpec::document(t!("plugins.title").to_string(), 940., 760.).min_size(660., 500.);
    let chrome = spec.chrome();
    let options = child_window_options(&spec, parent, cx);
    let view_app = app.clone();
    let close_app = app.downgrade();
    let result: anyhow::Result<NyaWindowHandle> = cx.open_window(options, move |window, cx| {
        window.on_window_should_close(cx, move |_, cx| {
            let _ = close_app.update(cx, |app, cx| {
                app.plugins.window.clear();
                if let Some(parent) = parent {
                    cx.defer(move |cx| {
                        let _ = parent.update(cx, |_, window, _| window.activate_window());
                    });
                }
            });
            true
        });
        let process = view_app.read(cx).plugins.process.clone();
        let panel = cx.new(|cx| PluginPanel::new(view_app.downgrade(), process, chrome, cx));
        cx.new(|cx| nya_root(panel, window, cx))
    });
    app.update(cx, |app, cx| {
        match result {
            Ok(handle) => app.plugins.window.finish_open(handle),
            Err(_) => {
                app.plugins.window.fail_open();
                app.shell.set_status(t!("plugins.openFailed").to_string());
            }
        }
        cx.notify();
    });
}
