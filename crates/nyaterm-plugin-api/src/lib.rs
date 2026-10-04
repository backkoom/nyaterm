//! Minimal guest-only SDK. The same WIT is consumed by Wasmtime in the host.
pub mod bindings {
    wit_bindgen::generate!({ path: "wit", world: "plugin", pub_export_macro: true });
}

pub use bindings::nyaterm::plugin::types::{
    ActionInput, ActionResult, Argument, CommandDraft, ErrorKind, Identity, PluginError, Value,
    Version,
};

pub const API_VERSION: Version = Version {
    major: 1,
    minor: 0,
    patch: 0,
};

pub trait Plugin: Default {
    fn initialize(&mut self, _identity: Identity) -> Result<(), PluginError> {
        Ok(())
    }
    fn invoke(&mut self, input: ActionInput) -> Result<ActionResult, PluginError>;
    fn shutdown(&mut self) {}
}

/// Register one stateful guest. Wasmtime serializes all calls to this instance.
#[macro_export]
macro_rules! register_plugin {
    ($plugin:ty) => {
        #[cfg(target_arch = "wasm32")]
        #[used]
        #[unsafe(link_section = "nyaterm:plugin-api")]
        static NYATERM_PLUGIN_API_VERSION: [u8; 5] = *b"1.0.0";

        struct NyaTermGuest;
        std::thread_local! {
            static NYATERM_PLUGIN: std::cell::RefCell<$plugin> = std::cell::RefCell::new(<$plugin as Default>::default());
        }
        impl $crate::bindings::Guest for NyaTermGuest {
            fn api_version() -> $crate::Version { $crate::API_VERSION }
            fn initialize(identity: $crate::Identity) -> Result<(), $crate::PluginError> {
                NYATERM_PLUGIN.with(|p| $crate::Plugin::initialize(&mut *p.borrow_mut(), identity))
            }
            fn invoke(input: $crate::ActionInput) -> Result<$crate::ActionResult, $crate::PluginError> {
                NYATERM_PLUGIN.with(|p| $crate::Plugin::invoke(&mut *p.borrow_mut(), input))
            }
            fn shutdown() { NYATERM_PLUGIN.with(|p| $crate::Plugin::shutdown(&mut *p.borrow_mut())); }
        }
        $crate::bindings::export!(NyaTermGuest with_types_in $crate::bindings);
    };
}
