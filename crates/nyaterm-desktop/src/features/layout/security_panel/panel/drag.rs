use gpui::{AppContext as _, Context, IntoElement, Point, Render, Window, div, prelude::*, rgb};

use crate::features::NyaTermApp;
use crate::models::{SecurityAuthTab, SecurityDropTarget};
use crate::theme::ThemePalette;

#[derive(Clone)]
struct SecurityDragPayload {
    tab: SecurityAuthTab,
    id: String,
    label: String,
}

struct SecurityDragPreview {
    label: String,
    position: Point<gpui::Pixels>,
}

impl Render for SecurityDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.position.x)
            .top(self.position.y)
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(0x202938))
            .text_xs()
            .text_color(rgb(0xf1f5f9))
            .child(self.label.clone())
    }
}

pub(super) fn security_drag_handle(
    tab: SecurityAuthTab,
    id: String,
    label: String,
    palette: ThemePalette,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(format!("security-drag-{}-{id}", tab.label()))
        .debug_selector({
            let id = id.clone();
            move || format!("security-drag-{}-{id}", tab.label())
        })
        .flex_none()
        .cursor_move()
        .child(crate::features::view_widgets::mono_icon(
            "icons/drag.svg",
            rgb(palette.text_dimmed).into(),
            14.,
        ))
        .on_drag(
            SecurityDragPayload { tab, id, label },
            |payload, position, _, cx| {
                cx.new(|_| SecurityDragPreview {
                    label: payload.label.clone(),
                    position,
                })
            },
        )
}

pub(super) fn security_sortable_row(
    row: gpui::Div,
    tab: SecurityAuthTab,
    id: String,
    palette: ThemePalette,
    app: &NyaTermApp,
    cx: &mut Context<NyaTermApp>,
) -> gpui::Div {
    let target = app
        .security
        .drop_target()
        .filter(|target| target.tab == tab && target.id == id);
    let before = target.is_some_and(|target| !target.after);
    let after = target.is_some_and(|target| target.after);
    row.debug_selector({
        let id = id.clone();
        move || format!("security-row-{}-{id}", tab.label())
    })
    .when(before, |row| {
        row.border_t_2().border_color(rgb(palette.link))
    })
    .when(after, |row| {
        row.border_b_2().border_color(rgb(palette.link))
    })
    .on_drag_move(cx.listener({
        let target_id = id.clone();
        move |this, event: &gpui::DragMoveEvent<SecurityDragPayload>, _, cx| {
            let payload = event.drag(cx);
            if payload.tab != tab || tab != this.security.auth_tab() || this.security.reorder_busy()
            {
                return;
            }
            let after =
                event.event.position.y >= event.bounds.origin.y + event.bounds.size.height / 2.;
            this.security.set_drop_target(Some(SecurityDropTarget {
                tab,
                id: target_id.clone(),
                after,
            }));
            this.ensure_drop_hover_clock(cx);
            cx.notify();
        }
    }))
    .on_drop(
        cx.listener(move |this, payload: &SecurityDragPayload, _, cx| {
            if payload.tab != tab {
                this.security.clear_drop_target();
                cx.notify();
                return;
            }
            let after = this
                .security
                .drop_target()
                .filter(|target| target.tab == tab && target.id == id)
                .is_some_and(|target| target.after);
            this.reorder_security_entries(tab, payload.id.clone(), id.clone(), after, cx);
        }),
    )
}

#[cfg(test)]
mod tests;
