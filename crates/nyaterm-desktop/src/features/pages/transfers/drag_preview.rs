use gpui::{Context, IntoElement, Pixels, Point, Render, Window, div, prelude::*, px, rgb};

use crate::theme::ThemePalette;

pub(super) struct TransferDragPreview {
    pub(super) label: String,
    pub(super) position: Point<Pixels>,
    pub(super) palette: ThemePalette,
}

impl Render for TransferDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.position.x + px(12.))
            .pt(self.position.y + px(12.))
            .child(
                div()
                    .debug_selector(|| "transfer-file-drag-preview".into())
                    .w(px(240.))
                    .h(px(36.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(self.palette.primary))
                    .bg(rgb(self.palette.surface_elevated))
                    .shadow_lg()
                    .text_size(px(12.))
                    .text_color(rgb(self.palette.text))
                    .child(crate::features::view_widgets::mono_icon(
                        "icons/fe/download.svg",
                        rgb(self.palette.primary).into(),
                        14.,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(self.label.clone()),
                    ),
            )
    }
}
