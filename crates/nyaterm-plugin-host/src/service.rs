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
        expected_revision: u64,
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

#[cfg(test)]
#[derive(Default)]
struct TestHooks {
    before_dispatch: Option<DispatchHook>,
    after_start: Option<DispatchHook>,
}
#[cfg(test)]
type DispatchHook = Arc<dyn Fn(&PluginOperation) + Send + Sync>;

fn shutdown_error() -> PluginError {
    PluginError::new(ErrorCode::Shutdown, "Plugin service is shutting down")
}

/// One process-level service. Startup, filesystem, preferences and compilation
/// run on this actor; each component has its own execution worker.
pub struct PluginService {
    requests: SyncSender<Request>,
    snapshot: Arc<RwLock<CatalogSnapshot>>,
    stop: Arc<AtomicBool>,
    // Linearizes submission, dispatch start and shutdown. Never held during I/O,
    // guest execution or join, so shutdown cannot strand a late accepted request.
    dispatch: Arc<Mutex<()>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl PluginService {
    pub fn start(
        runtime: AppRuntime,
    ) -> PluginResult<(Arc<Self>, async_channel::Receiver<PluginEvent>)> {
        Self::start_inner(
            runtime,
            #[cfg(test)]
            crate::runtime::RuntimeLimits::default(),
            #[cfg(test)]
            TestHooks::default(),
        )
    }

    fn start_inner(
        runtime: AppRuntime,
        #[cfg(test)] limits: crate::runtime::RuntimeLimits,
        #[cfg(test)] hooks: TestHooks,
    ) -> PluginResult<(Arc<Self>, async_channel::Receiver<PluginEvent>)> {
        let (requests, receiver) = mpsc::sync_channel::<Request>(16);
        let (mut events, event_receiver) = async_channel::channel(16);
        let snapshot = Arc::new(RwLock::new(CatalogSnapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let dispatch = Arc::new(Mutex::new(()));
        let snapshot_for_worker = snapshot.clone();
        let stop_for_worker = stop.clone();
        let dispatch_for_worker = dispatch.clone();
        let worker = thread::Builder::new()
            .name("plugin-manager".into())
            .spawn(move || {
                #[cfg(not(test))]
                let opened = PluginManager::for_runtime(&runtime);
                #[cfg(test)]
                let opened = PluginManager::open(
                    runtime.data_dir().join("plugins"),
                    env!("CARGO_PKG_VERSION"),
                    limits,
                );
                let mut manager = match opened {
                    Ok(manager) => Some(manager),
                    Err(error) => {
                        snapshot_for_worker.write().unwrap().startup_error = Some(error);
                        let _ = events.try_send(PluginEvent::CatalogChanged);
                        None
                    }
                };
                while !stop_for_worker.load(Ordering::Acquire) {
                    if let Some(manager) = &manager {
                        publish(manager, &snapshot_for_worker, &mut events);
                    }
                    let request = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    #[cfg(test)]
                    if let Some(hook) = &hooks.before_dispatch {
                        hook(&request.operation);
                    }
                    {
                        let _dispatch = dispatch_for_worker.lock().unwrap();
                        if stop_for_worker.load(Ordering::Acquire) {
                            let _ = request.reply.send(Err(shutdown_error()));
                            break;
                        }
                        if matches!(&request.operation, PluginOperation::Invoke { .. })
                            && request.reply.is_canceled()
                        {
                            continue;
                        }
                        // From here a management transaction is started and must
                        // finish or roll back even if shutdown begins next.
                    }
                    #[cfg(test)]
                    if let Some(hook) = &hooks.after_start {
                        hook(&request.operation);
                    }
                    let Some(manager) = &mut manager else {
                        let error = snapshot_for_worker
                            .read()
                            .unwrap()
                            .startup_error
                            .clone()
                            .unwrap();
                        let _ = request.reply.send(Err(error));
                        continue;
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
                            expected_revision,
                            input,
                        } => manager
                            .invoke_at_revision(&contribution_id, expected_revision, input)
                            .map(OperationReply::Invocation),
                    };
                    publish(manager, &snapshot_for_worker, &mut events);
                    let _ = request.reply.send(result);
                }
                // Submission is closed before draining, including any request
                // accepted immediately before shutdown acquired the dispatch lock.
                {
                    let _dispatch = dispatch_for_worker.lock().unwrap();
                    stop_for_worker.store(true, Ordering::Release);
                }
                while let Ok(request) = receiver.try_recv() {
                    let _ = request.reply.send(Err(shutdown_error()));
                }
                let final_snapshot = if let Some(mut manager) = manager.take() {
                    manager.shutdown();
                    let snapshot = manager.snapshot();
                    drop(manager); // Includes joining the epoch clock.
                    snapshot
                } else {
                    CatalogSnapshot {
                        stopped: true,
                        ..snapshot_for_worker.read().unwrap().clone()
                    }
                };
                *snapshot_for_worker.write().unwrap() = final_snapshot;
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
                dispatch,
                worker: Mutex::new(Some(worker)),
            }),
            event_receiver,
        ))
    }

    pub fn snapshot(&self) -> CatalogSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    pub fn submit(&self, operation: PluginOperation) -> PluginResult<OperationTask> {
        let _dispatch = self.dispatch.lock().unwrap();
        if self.stop.load(Ordering::Acquire) {
            return Err(shutdown_error());
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
        self.begin_shutdown();
        // Keep the join lock until completion: concurrent shutdown callers must
        // not return while the first caller is still reclaiming workers.
        let mut worker = self.worker.lock().unwrap();
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }

    fn begin_shutdown(&self) {
        let _dispatch = self.dispatch.lock().unwrap();
        self.stop.store(true, Ordering::Release);
    }
}

fn publish(
    manager: &PluginManager,
    snapshot: &RwLock<CatalogSnapshot>,
    events: &mut async_channel::Sender<PluginEvent>,
) {
    let current = manager.snapshot();
    if *snapshot.read().unwrap() != current {
        *snapshot.write().unwrap() = current;
        // Coalescing is safe: readers always fetch the latest snapshot.
        let _ = events.try_send(PluginEvent::CatalogChanged);
    }
}

impl Drop for PluginService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
