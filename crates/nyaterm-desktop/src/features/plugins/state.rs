use crate::features::NyaTermApp;
use crate::features::plugins::process::PluginProcess;
use crate::features::plugins::view::PluginPanel;
use gpui::{AppContext, Context, Entity, WeakEntity};

pub(in crate::features) struct PluginFeatureState {
    // Keep the editing state alive when the side panel is hidden or moved.
    pub panel: Entity<PluginPanel>,
}

impl PluginFeatureState {
    pub fn new(
        app: WeakEntity<NyaTermApp>,
        process: Entity<PluginProcess>,
        cx: &mut Context<NyaTermApp>,
    ) -> Self {
        let panel = cx.new(|cx| PluginPanel::new(app, process, cx));
        Self { panel }
    }
}
