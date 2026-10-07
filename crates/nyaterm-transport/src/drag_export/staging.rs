use crate::download_path::validate_name;
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, SystemTime},
};

/// Failed preparations are removed by TempDir. Successful URI sources survive
/// application exit because receivers may still be copying them asynchronously.
pub struct DragStagingDirectory(tempfile::TempDir);
impl DragStagingDirectory {
    pub fn new() -> anyhow::Result<Self> {
        cleanup_expired()?;
        Ok(Self(
            tempfile::Builder::new()
                .prefix("nyaterm-drag-export-")
                .tempdir()?,
        ))
    }
    pub fn targets(&self, names: impl IntoIterator<Item = String>) -> anyhow::Result<Vec<PathBuf>> {
        let mut used = HashSet::new();
        names
            .into_iter()
            .map(|name| {
                validate_name(&name)?;
                anyhow::ensure!(
                    used.insert(crate::download_path::target_key(&name)),
                    "duplicate drag filename"
                );
                Ok(self.0.path().join(name))
            })
            .collect()
    }
    pub fn retain(self) {
        let _retained_path = self.0.keep();
    }
}
fn cleanup_expired() -> anyhow::Result<()> {
    let now = SystemTime::now();
    for entry in std::fs::read_dir(std::env::temp_dir())? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(suffix) = name
            .to_str()
            .and_then(|name| name.strip_prefix("nyaterm-drag-export-"))
        else {
            continue;
        };
        // Only the six-character tempfile namespace belongs to this adapter.
        if suffix.len() != 6 || !suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        if now.duration_since(metadata.modified()?).unwrap_or_default()
            > Duration::from_secs(7 * 24 * 60 * 60)
        {
            // The path is a verified direct child of the system temporary directory.
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::DragStagingDirectory;
    #[test]
    fn failed_preparation_removes_partial_tree_and_rejects_unsafe_targets() {
        let staging = DragStagingDirectory::new().unwrap();
        let root = staging.0.path().to_path_buf();
        let targets = staging
            .targets(["folder".into(), "empty.txt".into()])
            .unwrap();
        std::fs::create_dir(&targets[0]).unwrap();
        std::fs::write(&targets[1], []).unwrap();
        assert!(staging.targets(["../escape".into()]).is_err());
        assert!(staging.targets(["same".into(), "same".into()]).is_err());
        drop(staging);
        assert!(!root.exists());
    }
}
