use crate::drag_export::RemoteDragFile;
use crate::{RemoteFileService, SftpFileEntry, SftpFileType, SftpTransferControl};
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, UNIX_EPOCH},
};

/// Selecting an ancestor and its child in a tree exports the ancestor once.
/// Compare wire identities so lossy display names cannot hide distinct sources.
pub fn prune_nested_roots(roots: Vec<SftpFileEntry>) -> anyhow::Result<Vec<SftpFileEntry>> {
    anyhow::ensure!(roots.len() <= 1024, "too many drag roots");
    let identities = roots
        .iter()
        .map(|entry| {
            let path = entry.remote_path();
            path.raw_path()
                .map(|raw| raw.unwrap_or_else(|| path.display_path.into_bytes()))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(roots
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            !roots.iter().enumerate().any(|(parent_index, parent)| {
                if parent_index == *index || parent.file_type != SftpFileType::Directory {
                    return false;
                }
                let mut prefix = identities[parent_index].clone();
                while prefix.len() > 1 && prefix.last() == Some(&b'/') {
                    prefix.pop();
                }
                if !prefix.ends_with(b"/") {
                    prefix.push(b'/');
                }
                identities[*index].starts_with(&prefix)
            })
        })
        .map(|(_, entry)| entry.clone())
        .collect())
}

#[derive(Clone, Debug)]
pub struct RemoteDragEntry {
    pub relative_path: PathBuf,
    pub file: RemoteDragFile,
    pub is_directory: bool,
}

/// Enumerate metadata on a worker; files remain deferred and empty directories
/// have descriptors of their own. Raw remote paths come from the SFTP listing.
pub fn enumerate_remote_drag(
    service: &RemoteFileService,
    roots: Vec<SftpFileEntry>,
    control: &SftpTransferControl,
) -> anyhow::Result<Vec<RemoteDragEntry>> {
    let mut pending: Vec<_> = roots
        .into_iter()
        .rev()
        .map(|entry| {
            let relative = PathBuf::from(&entry.name);
            (entry, relative, 0_usize)
        })
        .collect();
    let mut entries = Vec::new();
    let mut visited = HashSet::new();
    while let Some((entry, relative_path, depth)) = pending.pop() {
        control.check_cancelled()?;
        anyhow::ensure!(
            depth <= 128 && entries.len() < 65536,
            "drag directory tree is too large"
        );
        crate::download_path::validate_name(&entry.name)?;
        anyhow::ensure!(
            matches!(
                entry.file_type,
                SftpFileType::File | SftpFileType::Directory
            ),
            "links and special files cannot be dragged out"
        );
        let remote_path = entry.remote_path();
        anyhow::ensure!(
            visited.insert(remote_path.clone()),
            "duplicate remote drag source"
        );
        let metadata = service.remote_file_properties(&remote_path)?;
        anyhow::ensure!(
            metadata.file_type == entry.file_type,
            "remote drag source changed type"
        );
        let is_directory = entry.file_type == SftpFileType::Directory;
        if is_directory {
            let children = service.list_dir_path(&remote_path)?;
            anyhow::ensure!(
                children.len() + pending.len() + entries.len() <= 65536,
                "drag directory tree is too large"
            );
            control.check_cancelled()?;
            // Match ordinary directory downloads: do not follow links or copy devices.
            for child in children.into_iter().rev() {
                if crate::download_path::validate_directory_entry(&child.name, child.file_type)? {
                    let child_relative = relative_path.join(&child.name);
                    pending.push((child, child_relative, depth + 1));
                }
            }
        }
        entries.push(RemoteDragEntry {
            relative_path,
            is_directory,
            file: RemoteDragFile {
                display_name: entry.name.into(),
                remote_path,
                size: if is_directory { None } else { metadata.size },
                modified_at: metadata
                    .modified_at
                    .map(|time| UNIX_EPOCH + Duration::from_secs(u64::from(time))),
            },
        });
    }
    anyhow::ensure!(!entries.is_empty(), "no files selected");
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::prune_nested_roots;
    use crate::{RemoteFilePath, SftpFileEntry, SftpFileType};
    fn entry(raw: &[u8], directory: bool) -> SftpFileEntry {
        SftpFileEntry {
            name: "display".into(),
            path: "/same-display".into(),
            file_type: if directory {
                SftpFileType::Directory
            } else {
                SftpFileType::File
            },
            size: None,
            permissions: None,
            owner: String::new(),
            group: String::new(),
            modified_at: None,
            raw_path_token: RemoteFilePath::from_raw("/same-display", raw).raw_path_token,
            symlink_target_is_directory: false,
        }
    }
    #[test]
    fn ancestor_selection_removes_only_descendants_with_the_same_wire_identity() {
        let roots = vec![
            entry(b"/folder/child", false),
            entry(b"/folder", true),
            entry(b"/folder-other", false),
            entry(b"/other-\xff/child", false),
        ];
        let pruned = prune_nested_roots(roots.clone()).unwrap();
        assert_eq!(pruned, roots[1..]);
        assert_eq!(
            prune_nested_roots(vec![entry(b"/", true), entry(b"/child", false)])
                .unwrap()
                .len(),
            1
        );
    }
}
