use gpui::{
    AppContext as _, Context, Entity, IntoElement, MouseDownEvent, MouseMoveEvent, Render,
    TestAppContext, Window, div, point, prelude::*, px,
};
use nyaterm_core::{AppRuntime, RuntimeMode, test_support::TestTempDir};

use crate::entities::{OverlayStore, StartupRestoreStore, UiStoreHandles};
use crate::features::NyaTermApp;
use crate::models::BottomPanelMode;
use crate::send_command::SendCommandDataType;

struct CommandPanelFixture {
    app: Entity<NyaTermApp>,
    width: f32,
}

impl Render for CommandPanelFixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let workspace = self
            .app
            .update(cx, |app, cx| app.workspace_view(cx).into_any_element());
        div()
            .w(px(self.width))
            .h(px(760.))
            .flex()
            .flex_col()
            .child(workspace)
    }
}

#[test]
fn command_panel_keeps_editor_and_action_inside_empty_workspace_at_all_sizes() {
    let root = TestTempDir::new("nyaterm-command-panel-layout");
    let mut cx = TestAppContext::single();
    let runtime = AppRuntime::from_parts_for_test(
        RuntimeMode::Portable,
        root.path().to_path_buf(),
        root.path().join("config"),
        root.path().join("logs"),
        root.path().join("cache"),
        None,
    );
    let stores = UiStoreHandles {
        startup_restore: cx.new(|_| StartupRestoreStore::default()),
        overlays: cx.new(|_| OverlayStore::default()),
    };
    let app = cx.new(|cx| NyaTermApp::new(runtime, stores, cx));
    app.update(&mut cx, |app, cx| {
        app.sync_component_theme(cx);
        app.set_bottom_panel_mode(BottomPanelMode::CommandSend);
    });
    let fixture_app = app.clone();
    let (fixture, cx) = cx.add_window_view(move |_, cx| {
        cx.observe(&fixture_app, |_, _, cx| cx.notify()).detach();
        CommandPanelFixture {
            app: fixture_app,
            width: 1200.,
        }
    });
    for width in [1200., 420.] {
        fixture.update(cx, |fixture, cx| {
            fixture.width = width;
            cx.notify();
        });
        for data_type in [SendCommandDataType::Text, SendCommandDataType::Hex] {
            for height in [120., 180., 520., 120.] {
                app.update(cx, |app, cx| {
                    let delta = app.shell.command_send_height() - height;
                    app.start_bottom_panel_resize(&MouseDownEvent::default(), cx);
                    app.update_bottom_panel_resize(
                        &MouseMoveEvent {
                            position: point(px(0.), px(delta)),
                            ..Default::default()
                        },
                        cx,
                    );
                    app.finish_bottom_panel_resize(cx);
                    app.set_send_command_data_type(data_type, cx);
                    cx.notify();
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                let controls = cx.debug_bounds("bottom-command-controls").unwrap();
                let editor = cx.debug_bounds("send-command.draft").unwrap();
                let action = cx.debug_bounds("bottom-command-floating-send").unwrap();
                assert_eq!(controls.size.height, px(32.));
                assert!(editor.origin.y >= controls.bottom());
                assert!(editor.size.height >= px(44.));
                assert!(editor.bottom() <= px(760.));
                assert!(action.bottom() <= px(760.));
                assert!(action.origin.y >= editor.origin.y);
                assert!(action.origin.x >= editor.origin.x);
                assert!(action.right() <= editor.right());
                assert!(action.bottom() <= editor.bottom());
                assert!(action.right() <= px(width));
            }
        }
    }
}
