use crate::manager::{CatalogSnapshot, Invocation, PluginManager};
use futures::channel::{mpsc as async_channel, oneshot};
use nyaterm_core::plugins::invocation::ActionInput;
use nyaterm_core::plugins::{ErrorCode, PluginError, PluginResult};
use nyaterm_core::runtime::AppRuntime;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender, TrySendError},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub enum PluginOperation {
    Install {
        source: PathBuf,
        development: bool,
    },
    Update {
        id: String,
        source: PathBuf,
    },
    Enable {
        id: String,
        enabled: bool,
    },
    Reload {
        id: String,
    },
    Uninstall {
        id: String,
    },
    Invoke {
        contribution_id: String,
        input: ActionInput,
    },
}
pub enum OperationReply {
    Changed,
    Invocation(Invocation),
}
pub enum PluginEvent {
    CatalogChanged,
}
pub type OperationTask = oneshot::Receiver<PluginResult<OperationReply>>;
struct Request {
    operation: PluginOperation,
    reply: oneshot::Sender<PluginResult<OperationReply>>,
}

/// One process-level service. Startup, filesystem, preferences and compilation
/// run on this actor; each component has its own execution worker.
pub struct PluginService {
    requests: SyncSender<Request>,
    snapshot: Arc<RwLock<CatalogSnapshot>>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PluginService {
    pub fn start(
        runtime: AppRuntime,
    ) -> PluginResult<(Arc<Self>, async_channel::Receiver<PluginEvent>)> {
        let (requests, receiver) = mpsc::sync_channel::<Request>(16);
        let (mut events, event_receiver) = async_channel::channel(16);
        let snapshot = Arc::new(RwLock::new(CatalogSnapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let snapshot_for_worker = snapshot.clone();
        let stop_for_worker = stop.clone();
        let worker = thread::Builder::new()
            .name("plugin-manager".into())
            .spawn(move || {
                let mut manager = match PluginManager::for_runtime(&runtime) {
                    Ok(manager) => manager,
                    Err(error) => {
                        snapshot_for_worker.write().unwrap().startup_error = Some(error.clone());
                        let _ = events.try_send(PluginEvent::CatalogChanged);
                        while !stop_for_worker.load(Ordering::Acquire) {
                            if let Ok(request) = receiver.recv_timeout(Duration::from_millis(100)) {
                                let _ = request.reply.send(Err(error.clone()));
                            }
                        }
                        return;
                    }
                };
                while !stop_for_worker.load(Ordering::Acquire) {
                    let current = manager.snapshot();
                    if *snapshot_for_worker.read().unwrap() != current {
                        *snapshot_for_worker.write().unwrap() = current;
                        // Coalescing is safe: readers always fetch the latest snapshot.
                        let _ = events.try_send(PluginEvent::CatalogChanged);
                    }
                    let request = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let result = match request.operation {
                        PluginOperation::Install {
                            source,
                            development,
                        } => manager
                            .install(&source, development)
                            .map(|_| OperationReply::Changed),
                        PluginOperation::Update { id, source } => manager
                            .update(&id, &source)
                            .map(|_| OperationReply::Changed),
                        PluginOperation::Enable { id, enabled } => manager
                            .set_enabled(&id, enabled)
                            .map(|_| OperationReply::Changed),
                        PluginOperation::Reload { id } => {
                            manager.reload(&id).map(|_| OperationReply::Changed)
                        }
                        PluginOperation::Uninstall { id } => {
                            manager.uninstall(&id).map(|_| OperationReply::Changed)
                        }
                        PluginOperation::Invoke {
                            contribution_id,
                            input,
                        } => manager
                            .invoke(&contribution_id, input)
                            .map(OperationReply::Invocation),
                    };
                    let current = manager.snapshot();
                    if *snapshot_for_worker.read().unwrap() != current {
                        *snapshot_for_worker.write().unwrap() = current;
                        let _ = events.try_send(PluginEvent::CatalogChanged);
                    }
                    let _ = request.reply.send(result);
                }
                manager.shutdown();
                *snapshot_for_worker.write().unwrap() = manager.snapshot();
                let _ = events.try_send(PluginEvent::CatalogChanged);
            })
            .map_err(|_| {
                PluginError::new(
                    ErrorCode::Initialization,
                    "Cannot start plugin management worker",
                )
            })?;
        Ok((
            Arc::new(Self {
                requests,
                snapshot,
                stop,
                worker: Mutex::new(Some(worker)),
            }),
            event_receiver,
        ))
    }

    pub fn snapshot(&self) -> CatalogSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    pub fn submit(&self, operation: PluginOperation) -> PluginResult<OperationTask> {
        if self.stop.load(Ordering::Acquire) {
            return Err(PluginError::new(
                ErrorCode::Shutdown,
                "Plugin service is shutting down",
            ));
        }
        let (reply, task) = oneshot::channel();
        self.requests
            .try_send(Request { operation, reply })
            .map_err(|error| match error {
                TrySendError::Full(_) => PluginError::new(
                    ErrorCode::QueueFull,
                    "Plugin management queue is full; wait and retry",
                ),
                TrySendError::Disconnected(_) => PluginError::new(
                    ErrorCode::Shutdown,
                    "Plugin management worker is unavailable",
                ),
            })?;
        Ok(task)
    }

    /// Call off the UI thread, before AppShell quits the process.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

impl Drop for PluginService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
