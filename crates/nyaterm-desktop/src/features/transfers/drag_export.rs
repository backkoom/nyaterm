use crate::{
    features::NyaTermApp,
    models::{TransferJobEvent, TransferJobOutput, TransferJobResult},
};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, ExternalDragPayload, FileDragPaths, VirtualFileDescriptor, VirtualFileDragPayload,
    VirtualFileProvider, VirtualFileStream,
};
use nyaterm_transport::drag_export::{
    DragExportSource, ExportObserver, ExportReadEvent, RemoteDragFile, SftpReadSource,
    SftpReadStream,
};
use nyaterm_transport::{
    FileBrowserBackendKind, RemoteFileService, SftpFileEntry, SftpFileType, SftpTransferOptions,
    SftpTransferProgress, SftpTransferSummary,
};
use std::{
    cell::OnceCell,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Weak},
    time::{Duration, UNIX_EPOCH},
};

/// Immutable gesture snapshot: switching tabs or updating selection after the
/// drag starts cannot redirect its source connection or remote path.
#[derive(Clone)]
pub(in crate::features) struct TransferSelection {
    session_id: String,
    backend: FileBrowserBackendKind,
    entries: Vec<SftpFileEntry>,
    transfer_options: SftpTransferOptions,
    source_service: Option<Weak<RemoteFileService>>,
}

#[derive(Clone)]
pub(in crate::features) struct DraggedSelection {
    pub anchor: String,
    pub selection: Rc<OnceCell<TransferSelection>>,
}
impl DraggedSelection {
    pub fn new(anchor: String) -> Self {
        Self {
            anchor,
            selection: Rc::new(OnceCell::new()),
        }
    }
    pub fn file_count(&self) -> usize {
        self.selection
            .get()
            .map_or(0, |selection| selection.entries.len())
    }
}

pub(in crate::features) struct TransferDragExportService;

impl TransferDragExportService {
    fn sources(selection: &TransferSelection) -> anyhow::Result<Vec<DragExportSource>> {
        anyhow::ensure!(!selection.entries.is_empty(), "no files selected");
        selection
            .entries
            .iter()
            .map(|entry| {
                if selection.backend == FileBrowserBackendKind::Local {
                    Ok(DragExportSource::Local {
                        path: PathBuf::from(&entry.path),
                        is_directory: entry.is_directory(),
                    })
                } else {
                    anyhow::ensure!(
                        entry.file_type == SftpFileType::File,
                        "Remote directories and links cannot be dragged out; use Download"
                    );
                    Ok(DragExportSource::RemoteFile(RemoteDragFile {
                        display_name: entry.name.clone().into(),
                        size: entry.size,
                        remote_path: entry.remote_path(),
                        modified_at: entry
                            .modified_at
                            .map(|time| UNIX_EPOCH + Duration::from_secs(u64::from(time))),
                    }))
                }
            })
            .collect()
    }

    fn resolve(
        selection: &TransferSelection,
        service: Option<Weak<RemoteFileService>>,
        sender: UnboundedSender<TransferJobResult>,
        virtual_supported: bool,
    ) -> anyhow::Result<ExternalDragPayload> {
        let sources = Self::sources(selection)?;
        if selection.backend == FileBrowserBackendKind::Local {
            return Ok(ExternalDragPayload::Files(FileDragPaths::new(
                sources.into_iter().filter_map(|source| {
                    if let DragExportSource::Local { path, is_directory } = source {
                        Some((path, is_directory))
                    } else {
                        None
                    }
                }),
            )));
        }
        anyhow::ensure!(
            virtual_supported,
            "Remote drag download is unavailable on this platform; use Download"
        );
        let service = service.ok_or_else(|| anyhow::anyhow!("source session is closed"))?;
        let mut files = Vec::with_capacity(sources.len());
        for source in sources {
            let DragExportSource::RemoteFile(file) = source else {
                anyhow::bail!("mixed local and remote selection is unsupported");
            };
            let session_id = selection.session_id.clone();
            let path = file.remote_path.clone();
            let observer_tx = sender.clone();
            let factory = Arc::new(move || {
                // Each FILECONTENTS open has a distinct job and cancellation control.
                let id = format!("drag-export-{}", nyaterm_core::uuid());
                let tx = observer_tx.clone();
                let session_id = session_id.clone();
                let path = path.clone();
                Arc::new(move |event| {
                    let event = match event {
                        ExportReadEvent::Opened(control) => TransferJobEvent::DragExportOpened {
                            session_id: session_id.clone(),
                            remote_path: path.display_path.clone(),
                            control,
                        },
                        ExportReadEvent::Progress { bytes, total } => {
                            TransferJobEvent::Progress(SftpTransferProgress {
                                remote_path: path.display_path.clone(),
                                local_path: PathBuf::new(),
                                bytes_transferred: bytes,
                                total_bytes: total,
                                item_count_completed: None,
                                item_count_total: None,
                            })
                        }
                        ExportReadEvent::Finished(result) => {
                            TransferJobEvent::Finished(result.map(|bytes| {
                                TransferJobOutput::Summary(SftpTransferSummary {
                                    remote_path: path.display_path.clone(),
                                    local_path: PathBuf::new(),
                                    bytes,
                                    skipped: false,
                                })
                            }))
                        }
                    };
                    let _ = tx.unbounded_send(TransferJobResult {
                        id: id.clone(),
                        event,
                    });
                }) as ExportObserver
            });
            files.push(VirtualFileDescriptor {
                name: file.display_name.clone(),
                size: file.size,
                modified_at: file.modified_at,
                provider: Arc::new(RemoteProvider(
                    SftpReadSource::new(service.clone(), file, factory)
                        .with_transfer_options(selection.transfer_options.clone()),
                )),
            });
        }
        Ok(ExternalDragPayload::VirtualFiles(
            VirtualFileDragPayload::new(files)?,
        ))
    }
}

struct RemoteProvider(SftpReadSource);
impl VirtualFileProvider for RemoteProvider {
    fn open(&self) -> std::io::Result<Box<dyn VirtualFileStream>> {
        Ok(Box::new(RemoteStream(self.0.open()?)))
    }
    fn cancel(&self) {
        self.0.cancel();
    }
}
struct RemoteStream(SftpReadStream);
impl VirtualFileStream for RemoteStream {
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.read_at(offset, buffer)
    }
    fn cancel(&self) {
        self.0.cancel();
    }
}

impl NyaTermApp {
    pub(in crate::features) fn capture_transfer_drag(
        &mut self,
        drag: &DraggedSelection,
        cx: &mut Context<Self>,
    ) {
        let Some(session_id) = self.session.active_id_owned() else {
            return;
        };
        let Some(backend) = self.session.active_file_browser_backend() else {
            return;
        };
        let browser = self.transfer.browser_view();
        let marked = browser.selected_remote_paths;
        let entries = browser
            .entries
            .iter()
            .filter(|entry| {
                if marked.contains(&drag.anchor) {
                    marked.contains(&entry.identity_key())
                } else {
                    entry.matches_identity(&drag.anchor)
                }
            })
            .cloned()
            .collect();
        let _ = drag.selection.set(TransferSelection {
            source_service: self.session.weak_remote_file_service(&session_id),
            session_id,
            backend,
            entries,
            transfer_options: self.sftp_transfer_options(),
        });
        self.transfer.clear_browser_drag_selection();
        self.transfer.clear_browser_rename_click();
        self.transfer.cancel_browser_pending_rename();
        cx.notify();
    }

    pub(in crate::features) fn resolve_transfer_drag(
        &mut self,
        drag: &DraggedSelection,
        virtual_supported: bool,
        cx: &mut Context<Self>,
    ) -> Option<ExternalDragPayload> {
        let selection = drag.selection.get()?;
        self.session.metadata(&selection.session_id)?;
        if selection.backend == FileBrowserBackendKind::Remote {
            let service = selection.source_service.as_ref().and_then(Weak::upgrade)?;
            if service.selected_backend() != Some(nyaterm_transport::RemoteFileBackendKind::Sftp) {
                self.shell
                    .set_status("Remote drag export requires SFTP; use Download".to_string());
                cx.notify();
                return None;
            }
        }
        match TransferDragExportService::resolve(
            selection,
            selection.source_service.clone(),
            self.transfer.transfer_event_sender(),
            virtual_supported,
        ) {
            Ok(payload) => Some(payload),
            Err(error) => {
                self.shell.set_status(error.to_string());
                cx.notify();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TransferDragExportService, TransferSelection};
    use nyaterm_transport::drag_export::DragExportSource;
    use nyaterm_transport::{FileBrowserBackendKind, SftpFileEntry, SftpFileType};
    fn entry(name: &str, kind: SftpFileType) -> SftpFileEntry {
        SftpFileEntry {
            name: name.into(),
            path: format!("/remote/{name}"),
            file_type: kind,
            size: Some(0),
            permissions: None,
            owner: String::new(),
            group: String::new(),
            modified_at: Some(17),
            raw_path_token: Some("L3JlbW90ZS9yYXc".into()),
            symlink_target_is_directory: false,
        }
    }
    #[test]
    fn remote_metadata_preserves_raw_path_and_has_no_destination() {
        let selection = TransferSelection {
            session_id: "session".into(),
            source_service: None,
            transfer_options: nyaterm_transport::SftpTransferOptions::default(),
            backend: FileBrowserBackendKind::Remote,
            entries: vec![entry("你好.txt", SftpFileType::File)],
        };
        let sources = TransferDragExportService::sources(&selection).unwrap();
        let DragExportSource::RemoteFile(file) = &sources[0] else {
            panic!();
        };
        assert_eq!(file.display_name, std::ffi::OsString::from("你好.txt"));
        assert_eq!(file.size, Some(0));
        assert_eq!(file.remote_path, selection.entries[0].remote_path());
        assert_eq!(
            file.modified_at
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            17
        );
    }
    #[test]
    fn remote_directory_or_link_rejects_whole_selection() {
        for kind in [
            SftpFileType::Directory,
            SftpFileType::Symlink,
            SftpFileType::Other,
        ] {
            let selection = TransferSelection {
                session_id: "session".into(),
                source_service: None,
                transfer_options: nyaterm_transport::SftpTransferOptions::default(),
                backend: FileBrowserBackendKind::Remote,
                entries: vec![entry("file", SftpFileType::File), entry("other", kind)],
            };
            assert!(TransferDragExportService::sources(&selection).is_err());
        }
    }
    #[test]
    fn local_selection_maps_to_real_paths_including_directories() {
        let selection = TransferSelection {
            session_id: "session".into(),
            source_service: None,
            transfer_options: nyaterm_transport::SftpTransferOptions::default(),
            backend: FileBrowserBackendKind::Local,
            entries: vec![entry("dir", SftpFileType::Directory)],
        };
        let (sender, _) = futures::channel::mpsc::unbounded();
        let gpui::ExternalDragPayload::Files(paths) =
            TransferDragExportService::resolve(&selection, None, sender, false).unwrap()
        else {
            panic!();
        };
        assert_eq!(
            paths.entries(),
            &[(std::path::PathBuf::from("/remote/dir"), true)]
        );
    }

    #[test]
    fn remote_selection_is_deferred_and_keeps_native_content_order() {
        let selection = TransferSelection {
            session_id: "session".into(),
            source_service: None,
            transfer_options: nyaterm_transport::SftpTransferOptions::default(),
            backend: FileBrowserBackendKind::Remote,
            entries: vec![
                entry("你好.txt", SftpFileType::File),
                entry("second", SftpFileType::File),
            ],
        };
        let (sender, mut receiver) = futures::channel::mpsc::unbounded();
        let gpui::ExternalDragPayload::VirtualFiles(files) = TransferDragExportService::resolve(
            &selection,
            Some(std::sync::Weak::new()),
            sender,
            true,
        )
        .unwrap() else {
            panic!("remote files must be deferred");
        };
        assert_eq!(files.files().len(), 2);
        assert_eq!(files.files()[0].name, std::ffi::OsString::from("你好.txt"));
        assert_eq!(files.files()[1].name, std::ffi::OsString::from("second"));
        assert!(receiver.try_recv().is_err());
        // Closing the source before a consumer opens it fails without networking.
        assert!(files.files()[0].provider.open().is_err());
        files.cancel();
        assert!(files.files()[1].provider.open().is_err());
    }

    #[test]
    fn unsafe_remote_names_and_unsupported_platform_reject_before_content() {
        for name in ["../escape", "NUL.txt", "name:stream"] {
            let selection = TransferSelection {
                session_id: "session".into(),
                source_service: None,
                transfer_options: nyaterm_transport::SftpTransferOptions::default(),
                backend: FileBrowserBackendKind::Remote,
                entries: vec![entry(name, SftpFileType::File)],
            };
            let (sender, _) = futures::channel::mpsc::unbounded();
            assert!(
                TransferDragExportService::resolve(
                    &selection,
                    Some(std::sync::Weak::new()),
                    sender,
                    true
                )
                .is_err()
            );
        }
        let selection = TransferSelection {
            session_id: "session".into(),
            source_service: None,
            transfer_options: nyaterm_transport::SftpTransferOptions::default(),
            backend: FileBrowserBackendKind::Remote,
            entries: vec![entry("file", SftpFileType::File)],
        };
        let (sender, _) = futures::channel::mpsc::unbounded();
        assert!(TransferDragExportService::resolve(&selection, None, sender, false).is_err());
    }

    #[test]
    fn gesture_snapshot_cannot_be_redirected_after_capture() {
        let drag = super::DraggedSelection::new("anchor".into());
        let service = std::sync::Arc::new(nyaterm_transport::RemoteFileService::new(
            nyaterm_transport::SshSessionConfig::default(),
        ));
        let selection = TransferSelection {
            session_id: "original".into(),
            source_service: Some(std::sync::Arc::downgrade(&service)),
            transfer_options: nyaterm_transport::SftpTransferOptions::default(),
            backend: FileBrowserBackendKind::Remote,
            entries: vec![entry("original", SftpFileType::File)],
        };
        assert!(drag.selection.set(selection.clone()).is_ok());
        let mut changed = selection;
        changed.session_id = "other".into();
        changed.source_service = None;
        changed.entries.clear();
        changed.transfer_options = changed.transfer_options.with_download_threads(9);
        assert!(drag.clone().selection.set(changed).is_err());
        assert_eq!(drag.selection.get().unwrap().session_id, "original");
        assert_eq!(drag.selection.get().unwrap().entries.len(), 1);
        assert_eq!(
            drag.selection
                .get()
                .unwrap()
                .transfer_options
                .download_threads(),
            3
        );
        assert!(std::sync::Weak::ptr_eq(
            drag.selection
                .get()
                .unwrap()
                .source_service
                .as_ref()
                .unwrap(),
            &std::sync::Arc::downgrade(&service),
        ));
    }
}
