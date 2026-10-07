use crate::models::{TransferJobEvent, TransferJobOutput, TransferJobResult};
use futures::channel::mpsc::UnboundedSender;
use gpui::PromisedFileProvider;
use nyaterm_transport::{
    RemoteFileService, SftpDuplicatePolicy, SftpFileEntry, SftpPathTransferOptions,
    SftpTransferControl, SftpTransferOptions,
};
use std::{
    io,
    path::Path,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) struct RemoteDownloadProvider {
    session_id: String,
    service: Weak<RemoteFileService>,
    entry: SftpFileEntry,
    options: SftpTransferOptions,
    sender: UnboundedSender<TransferJobResult>,
    control: SftpTransferControl,
    expose_destination: bool,
}
impl RemoteDownloadProvider {
    pub(super) fn new(
        session_id: String,
        service: Weak<RemoteFileService>,
        entry: SftpFileEntry,
        options: SftpTransferOptions,
        sender: UnboundedSender<TransferJobResult>,
        expose_destination: bool,
    ) -> Self {
        Self {
            session_id,
            service,
            entry,
            options,
            sender,
            control: SftpTransferControl::new(),
            expose_destination,
        }
    }
}
impl PromisedFileProvider for RemoteDownloadProvider {
    fn write_to(&self, target: &Path) -> io::Result<()> {
        let source = self
            .service
            .upgrade()
            .ok_or_else(|| io::Error::other("source session is closed"))?;
        // A snapshot owns the active operation; the authoritative connection can
        // still disappear and cancel it, even during a stalled network request.
        let service = (*source).clone();
        drop(source);
        let control = self.control.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let watcher = stopped.clone();
        let weak = self.service.clone();
        let watched_control = control.clone();
        std::thread::Builder::new()
            .name("drag-download-lifetime".into())
            .spawn(move || {
                while !watcher.load(Ordering::Acquire) {
                    if weak.upgrade().is_none() {
                        watched_control.cancel();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            })?;
        struct WatchGuard(Arc<AtomicBool>);
        impl Drop for WatchGuard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _watch_guard = WatchGuard(stopped);
        let remote_path = self.entry.remote_path();
        let id = format!("drag-download-{}", nyaterm_core::uuid());
        let send = |event| {
            let _ = self.sender.unbounded_send(TransferJobResult {
                id: id.clone(),
                event,
            });
        };
        send(TransferJobEvent::DragExportOpened {
            session_id: self.session_id.clone(),
            remote_path: remote_path.display_path.clone(),
            control: control.clone(),
            destination: self
                .expose_destination
                .then(|| (remote_path.raw_path_token.clone(), target.to_path_buf())),
        });
        let sender = self.sender.clone();
        let progress_id = id.clone();
        let expose_destination = self.expose_destination;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<_> {
                control.check_cancelled()?;
                let name = target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| anyhow::anyhow!("invalid promised destination"))?;
                nyaterm_transport::download_path::validate_name(name)?;
                let metadata = service.remote_file_properties(&remote_path)?;
                anyhow::ensure!(
                    metadata.file_type == self.entry.file_type,
                    "remote drag source changed type"
                );
                // Finder resolves top-level naming conflicts. A race must never turn
                // its negotiated promise into an overwrite of another user's file.
                let mut summary = service.download_remote_path_with_progress_and_path_options(
                    &remote_path,
                    target.to_path_buf(),
                    control,
                    SftpPathTransferOptions::new(
                        SftpDuplicatePolicy::Skip,
                        None,
                        self.options.clone(),
                    ),
                    move |mut progress| {
                        if !expose_destination {
                            progress.local_path.clear();
                        }
                        let _ = sender.unbounded_send(TransferJobResult {
                            id: progress_id.clone(),
                            event: TransferJobEvent::Progress(progress),
                        });
                    },
                )?;
                anyhow::ensure!(!summary.skipped, "promised destination already exists");
                if !self.expose_destination {
                    summary.local_path.clear();
                }
                Ok(summary)
            }))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("drag download failed")))
            .map_err(|error| error.to_string());
        match result {
            Ok(summary) => {
                send(TransferJobEvent::Finished(Ok(TransferJobOutput::Summary(
                    summary,
                ))));
                Ok(())
            }
            Err(error) => {
                send(TransferJobEvent::Finished(Err(error.clone())));
                Err(io::Error::other(error))
            }
        }
    }
    fn cancel(&self) {
        self.control.cancel();
    }
}
impl Drop for RemoteDownloadProvider {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
