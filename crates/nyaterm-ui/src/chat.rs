//! Chat presentation primitives at the GPUI Kit integration boundary.

pub use gpui_kit::component::message_scroller::{
    MessageScroller as NyaMessageScroller, MessageScrollerState as NyaMessageScrollerState,
};
pub use gpui_kit::component::shimmer::ShimmerText as NyaShimmerText;

use gpui::{
    Animation, AnimationExt, IntoElement, SharedString, Transformation, percentage, prelude::*, px,
    svg,
};
use std::time::Duration;

/// A single subtle activity indicator; callers remove it when execution settles.
pub fn running_indicator(id: impl Into<SharedString>) -> impl IntoElement {
    svg()
        .path("icons/conn/spinner-arc.svg")
        .size(px(12.))
        .with_animation(
            id.into(),
            Animation::new(Duration::from_secs(2)).repeat(),
            |icon, progress| icon.with_transformation(Transformation::rotate(percentage(progress))),
        )
}
