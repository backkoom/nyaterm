use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nyaterm_core::plugins::invocation::{ActionInput, ActionResult, validate_input};
use nyaterm_core::plugins::manifest::{ActionManifest, PluginManifest};
use nyaterm_core::plugins::preferences::{PluginPreference, PluginPreferences};
use nyaterm_core::plugins::template::expand;
use nyaterm_core::plugins::{ErrorCode, PluginError, PluginResult, PluginStatus, validate_id};
use nyaterm_core::runtime::AppRuntime;
use nyaterm_store::plugin_preferences::PluginPreferenceStore;

use crate::package::{
    Package, StagingDirectory, io_error, load, promote, reject_ancestor_links, reject_link,
    snapshot,
};
use crate::runtime::{CallTicket, RuntimeEngine, RuntimeInstance, RuntimeLimits, RuntimeToken};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginSource {
    Managed,
    Development(PathBuf),
}

#[derive(Clone, PartialEq, Eq)]
pub struct PluginSnapshot {
    pub id: String,
    pub manifest: Option<PluginManifest>,
    pub enabled: bool,
    pub source: PluginSource,
    pub status: PluginStatus,
    pub revision: u64,
    pub error: Option<PluginError>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct Contribution {
    pub id: String,
    pub plugin_id: String,
    pub revision: u64,
    pub action: ActionManifest,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub plugins: Vec<PluginSnapshot>,
    pub contributions: Vec<Contribution>,
    pub startup_error: Option<PluginError>,
    pub stopped: bool,
}

struct Entry {
    manifest: Option<PluginManifest>,
    templates: BTreeMap<String, String>,
    instance: Option<Arc<RuntimeInstance>>,
    enabled: bool,
    source: PluginSource,
    revision: u64,
    valid: Arc<AtomicBool>,
    error: Option<PluginError>,
}

impl Entry {
    fn retire(&mut self) {
        self.valid.store(false, Ordering::Release);
        if let Some(instance) = self.instance.take() {
            instance.stop();
        }
    }
    fn usable(&self) -> bool {
        self.enabled
            && self.error.is_none()
            && self.instance.as_ref().is_none_or(|i| i.token().active())
    }
}
impl Drop for Entry {
    fn drop(&mut self) {
        self.retire();
    }
}

pub struct Invocation {
    pub plugin_id: String,
    pub contribution_id: String,
    pub revision: u64,
    lease: InvocationLease,
    pending: InvocationResult,
}
/// Read-only generation proof retained by previews. Only the registry/runtime
/// can revoke it; windows cannot revive a retired result.
#[derive(Clone)]
pub struct InvocationLease {
    valid: Arc<AtomicBool>,
    runtime: Option<Arc<RuntimeToken>>,
}
impl InvocationLease {
    pub fn is_current(&self) -> bool {
        self.valid.load(Ordering::Acquire)
            && self.runtime.as_ref().is_none_or(|token| token.active())
    }
}
enum InvocationResult {
    Ready(PluginResult<ActionResult>),
    Wasm(CallTicket),
}

impl Invocation {
    pub fn lease(&self) -> InvocationLease {
        self.lease.clone()
    }
    pub fn cancel(&self) {
        if let InvocationResult::Wasm(ticket) = &self.pending {
            ticket.cancel();
        }
    }
    pub async fn result(self) -> PluginResult<ActionResult> {
        let result = match self.pending {
            InvocationResult::Ready(result) => result,
            InvocationResult::Wasm(ticket) => ticket.result().await,
        };
        if !self.lease.valid.load(Ordering::Acquire) {
            return Err(PluginError::new(
                ErrorCode::Cancelled,
                "Plugin result belongs to a retired revision",
            ));
        }
        result
    }
}

/// All catalog mutations are serialized by the service's one management worker.
/// Windows only receive immutable snapshots, never a second mutable registry.
pub struct PluginManager {
    root: PathBuf,
    host_version: String,
    engine: Arc<RuntimeEngine>,
    limits: RuntimeLimits,
    store: PluginPreferenceStore,
    preferences: PluginPreferences,
    entries: BTreeMap<String, Entry>,
    revision: u64,
    stopped: bool,
}

impl PluginManager {
    pub fn for_runtime(runtime: &AppRuntime) -> PluginResult<Self> {
        Self::open(
            runtime.data_dir().join("plugins"),
            env!("CARGO_PKG_VERSION"),
            RuntimeLimits::default(),
        )
    }

    pub fn open(root: PathBuf, host_version: &str, limits: RuntimeLimits) -> PluginResult<Self> {
        reject_ancestor_links(&root)?;
        for directory in [
            &root,
            &root.join("installed"),
            &root.join("staging"),
            &root.join("work/data"),
            &root.join("dev"),
        ] {
            fs::create_dir_all(directory).map_err(|_| io_error())?;
            reject_ancestor_links(directory)?;
        }
        let preferences_path = root.join("dev/preferences.redb");
        if preferences_path.exists() {
            reject_link(&preferences_path)?;
        }
        let store = PluginPreferenceStore::open(&preferences_path)?;
        let preferences = store.load()?;
        let engine = RuntimeEngine::new()?;
        let mut manager = Self {
            root,
            host_version: host_version.into(),
            engine,
            limits,
            store,
            preferences,
            entries: BTreeMap::new(),
            revision: 0,
            stopped: false,
        };
        manager.discover()?;
        Ok(manager)
    }

    fn next_revision(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    fn prepare(
        &mut self,
        directory: &Path,
        enabled: bool,
        source: PluginSource,
    ) -> PluginResult<Entry> {
        let Package {
            manifest,
            templates,
            component,
        } = load(directory, &self.host_version)?;
        // Even disabled installations must prove that their actual ABI initializes.
        let instance = component
            .as_ref()
            .map(|bytes| {
                RuntimeInstance::prepare(self.engine.clone(), bytes, &manifest, self.limits.clone())
            })
            .transpose()?;
        if !enabled && let Some(instance) = &instance {
            instance.stop();
        }
        Ok(Entry {
            manifest: Some(manifest),
            templates,
            instance: if enabled { instance } else { None },
            enabled,
            source,
            revision: self.next_revision(),
            valid: Arc::new(AtomicBool::new(enabled)),
            error: None,
        })
    }

    fn discover(&mut self) -> PluginResult<()> {
        let paths = fs::read_dir(self.root.join("installed"))
            .map_err(|_| io_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| io_error())?;
        if paths.len() > 1024 {
            return Err(PluginError::new(
                ErrorCode::PackageLimit,
                "Too many installed plugins",
            ));
        }
        for path in paths {
            let id = path.file_name().to_str().ok_or_else(io_error)?.to_string();
            validate_id(&id)?;
            let preference = self
                .preferences
                .plugins
                .get(&id)
                .cloned()
                .unwrap_or_default();
            let source = preference
                .development_source
                .as_ref()
                .map_or(PluginSource::Managed, |p| {
                    PluginSource::Development(PathBuf::from(p))
                });
            let entry = match self.prepare(&path.path(), preference.enabled, source.clone()) {
                Ok(entry) if entry.manifest.as_ref().is_some_and(|m| m.id == id) => entry,
                result => Entry {
                    manifest: None,
                    templates: BTreeMap::new(),
                    instance: None,
                    enabled: preference.enabled,
                    source,
                    revision: self.next_revision(),
                    valid: Arc::new(AtomicBool::new(false)),
                    error: Some(result.err().unwrap_or_else(|| {
                        PluginError::new(
                            ErrorCode::Conflict,
                            "Installed directory does not match plugin ID",
                        )
                    })),
                },
            };
            self.entries.insert(id, entry);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> CatalogSnapshot {
        let mut snapshot = CatalogSnapshot {
            stopped: self.stopped,
            ..CatalogSnapshot::default()
        };
        for (id, entry) in &self.entries {
            let error = entry
                .error
                .clone()
                .or_else(|| entry.instance.as_ref().and_then(|i| i.token().fault()));
            let status = if let Some(error) = &error {
                if error.code == ErrorCode::Incompatible {
                    PluginStatus::Incompatible
                } else if entry.instance.is_some() {
                    PluginStatus::Faulted
                } else {
                    PluginStatus::LoadFailed
                }
            } else if entry.enabled {
                if entry.instance.is_some() {
                    PluginStatus::Running
                } else {
                    PluginStatus::Enabled
                }
            } else {
                PluginStatus::Installed
            };
            snapshot.plugins.push(PluginSnapshot {
                id: id.clone(),
                manifest: entry.manifest.clone(),
                enabled: entry.enabled,
                source: entry.source.clone(),
                status,
                revision: entry.revision,
                error,
            });
            if entry.usable()
                && let Some(manifest) = &entry.manifest
            {
                snapshot
                    .contributions
                    .extend(manifest.actions.iter().cloned().map(|action| Contribution {
                        id: manifest.contribution_id(&action),
                        plugin_id: id.clone(),
                        revision: entry.revision,
                        action,
                    }));
            }
        }
        snapshot
    }

    fn ensure_active(&self) -> PluginResult<()> {
        if self.stopped {
            Err(PluginError::new(
                ErrorCode::Shutdown,
                "Plugin service is shutting down",
            ))
        } else {
            Ok(())
        }
    }

    pub fn install(&mut self, source: &Path, development: bool) -> PluginResult<String> {
        self.replace(source, development, None)
    }

    pub fn update(&mut self, id: &str, source: &Path) -> PluginResult<String> {
        let development = matches!(
            self.entries.get(id).map(|e| &e.source),
            Some(PluginSource::Development(_))
        );
        self.replace(source, development, Some(id))
    }

    fn replace(
        &mut self,
        source: &Path,
        development: bool,
        expected: Option<&str>,
    ) -> PluginResult<String> {
        self.ensure_active()?;
        if development && !reject_link(source)?.is_dir() {
            return Err(PluginError::new(
                ErrorCode::InvalidManifest,
                "Register a development directory, not an archive",
            ));
        }
        let mut stage = StagingDirectory::new(&self.root.join("staging"))?;
        snapshot(source, &stage.path)?;
        let package = load(&stage.path, &self.host_version)?;
        let id = package.manifest.id;
        if expected.is_some_and(|expected| expected != id) {
            return Err(PluginError::new(
                ErrorCode::Conflict,
                "Update package has a different plugin ID",
            ));
        }
        if expected.is_none() && self.entries.contains_key(&id) {
            return Err(PluginError::new(
                ErrorCode::Conflict,
                "Plugin is already installed; use Update",
            ));
        }
        if expected.is_some() && !self.entries.contains_key(&id) {
            return Err(PluginError::new(
                ErrorCode::NotFound,
                "Plugin is not installed",
            ));
        }
        let enabled = self.entries.get(&id).is_none_or(|e| e.enabled);
        let source_kind = if development {
            PluginSource::Development(fs::canonicalize(source).map_err(|_| io_error())?)
        } else {
            PluginSource::Managed
        };
        let candidate = self.prepare(&stage.path, enabled, source_kind.clone())?;
        let target = self.root.join("installed").join(&id);
        let mut preferences = self.preferences.clone();
        preferences.plugins.insert(
            id.clone(),
            PluginPreference {
                enabled,
                development_source: match source_kind {
                    PluginSource::Managed => None,
                    PluginSource::Development(path) => Some(path.to_string_lossy().into_owned()),
                },
            },
        );
        promote(&mut stage, &target, &self.root.join("staging"), || {
            self.store.save(&preferences)
        })?;
        self.preferences = preferences;
        // Candidate owns bytes/Store, not package handles. Retirement cannot stop
        // the prepared candidate, and no old worker can publish into its lease.
        if let Some(mut old) = self.entries.insert(id.clone(), candidate) {
            old.retire();
        }
        Ok(id)
    }

    pub fn reload(&mut self, id: &str) -> PluginResult<()> {
        self.ensure_active()?;
        validate_id(id)?;
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| PluginError::new(ErrorCode::NotFound, "Plugin is not installed"))?;
        if let PluginSource::Development(source) = &entry.source {
            let source = source.clone();
            self.replace(&source, true, Some(id))?;
        } else {
            let candidate = self.prepare(
                &self.root.join("installed").join(id),
                entry.enabled,
                entry.source.clone(),
            )?;
            if let Some(mut old) = self.entries.insert(id.into(), candidate) {
                old.retire();
            }
        }
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> PluginResult<()> {
        self.ensure_active()?;
        validate_id(id)?;
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| PluginError::new(ErrorCode::NotFound, "Plugin is not installed"))?;
        let candidate = if enabled {
            Some(self.prepare(
                &self.root.join("installed").join(id),
                true,
                entry.source.clone(),
            )?)
        } else {
            None
        };
        let mut preferences = self.preferences.clone();
        preferences.plugins.entry(id.into()).or_default().enabled = enabled;
        self.store.save(&preferences)?;
        self.preferences = preferences;
        if let Some(candidate) = candidate {
            if let Some(mut old) = self.entries.insert(id.into(), candidate) {
                old.retire();
            }
        } else {
            let revision = self.next_revision();
            let entry = self.entries.get_mut(id).unwrap();
            entry.retire();
            entry.enabled = false;
            entry.revision = revision;
        }
        Ok(())
    }

    /// Uninstall revokes all contributions and retains work/data/<id> by policy.
    pub fn uninstall(&mut self, id: &str) -> PluginResult<()> {
        self.ensure_active()?;
        validate_id(id)?;
        if !self.entries.contains_key(id) {
            return Err(PluginError::new(
                ErrorCode::NotFound,
                "Plugin is not installed",
            ));
        }
        let mut trash = StagingDirectory::new(&self.root.join("staging"))?;
        let target = self.root.join("installed").join(id);
        reject_ancestor_links(&target)?;
        fs::rename(&target, trash.path.join("old")).map_err(|_| io_error())?;
        let mut preferences = self.preferences.clone();
        preferences.plugins.remove(id);
        if let Err(error) = self.store.save(&preferences) {
            if fs::rename(trash.path.join("old"), &target).is_err() {
                trash.retain();
                return Err(PluginError::new(
                    ErrorCode::Io,
                    "Uninstall rollback failed; the old package is retained in staging for recovery",
                ));
            }
            return Err(error);
        }
        self.preferences = preferences;
        if let Some(mut old) = self.entries.remove(id) {
            old.retire();
        }
        Ok(())
    }

    pub fn invoke(&self, contribution_id: &str, input: ActionInput) -> PluginResult<Invocation> {
        self.invoke_checked(contribution_id, None, input)
    }

    /// Check the caller's selected contribution before any input reaches a guest.
    pub fn invoke_at_revision(
        &self,
        contribution_id: &str,
        expected_revision: u64,
        input: ActionInput,
    ) -> PluginResult<Invocation> {
        self.invoke_checked(contribution_id, Some(expected_revision), input)
    }

    fn invoke_checked(
        &self,
        contribution_id: &str,
        expected_revision: Option<u64>,
        input: ActionInput,
    ) -> PluginResult<Invocation> {
        self.ensure_active()?;
        let (id, action_id) = contribution_id
            .split_once(':')
            .ok_or_else(|| PluginError::new(ErrorCode::NotFound, "Choose a plugin action"))?;
        let entry = self
            .entries
            .get(id)
            .ok_or_else(|| PluginError::new(ErrorCode::NotFound, "Plugin is not installed"))?;
        if expected_revision.is_some_and(|revision| revision != entry.revision) {
            return Err(PluginError::new(
                ErrorCode::Cancelled,
                "Plugin action changed; select it again before invoking",
            ));
        }
        if !entry.usable() {
            return Err(PluginError::new(
                ErrorCode::Disabled,
                "Plugin action is disabled or faulted; enable or reload it",
            ));
        }
        let action = entry
            .manifest
            .as_ref()
            .and_then(|m| m.actions.iter().find(|a| a.id == action_id))
            .ok_or_else(|| {
                PluginError::new(ErrorCode::NotFound, "Plugin action is not declared")
            })?;
        let input = validate_input(action, &input)?;
        let pending = if let Some(template) = entry.templates.get(action_id) {
            InvocationResult::Ready(expand(action, template, &input))
        } else {
            InvocationResult::Wasm(
                entry
                    .instance
                    .as_ref()
                    .ok_or_else(|| {
                        PluginError::new(
                            ErrorCode::Initialization,
                            "Plugin component is unavailable",
                        )
                    })?
                    .invoke(action_id, action.result, input)?,
            )
        };
        Ok(Invocation {
            plugin_id: id.into(),
            contribution_id: contribution_id.into(),
            revision: entry.revision,
            lease: InvocationLease {
                valid: entry.valid.clone(),
                runtime: entry.instance.as_ref().map(|instance| instance.token()),
            },
            pending,
        })
    }

    pub fn shutdown(&mut self) {
        self.stopped = true;
        for entry in self.entries.values_mut() {
            entry.retire();
        }
        self.entries.clear();
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
