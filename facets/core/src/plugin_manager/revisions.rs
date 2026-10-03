//! A plugin's files on disk, kept as revisions that share what did not change.
//!
//! Every time a plugin is loaded with new content, the files are compared by SHA-256
//! with the plugin's latest revision. Only the files that differ are written, into a new
//! folder named after the date and time:
//!
//! ```text
//! <app_dir>/plugins/<name>/
//! ├── 20261003T210100Z/     plugin.toml, pages/notes.xml, plugin.wasm, files.json
//! └── 20261003T213500Z/     plugin.wasm, files.json     (only the wasm changed)
//! ```
//!
//! `files.json` lists *every* file of the revision with its hash and the folder where its
//! bytes are, so a revision is complete without repeating files. Folders made before this
//! layout (`plugins/<name>/<version>/`, every file present) are read as revisions too.

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::catalog::sha256_hex;
use crate::app_dir::{AppDir, AppDirError};

pub const INDEX_FILE: &str = "files.json";

#[derive(Debug, Error)]
pub enum RevisionError {
    #[error("plugin file error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot read the file index {path}: {source}")]
    Index {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(transparent)]
    AppDir(#[from] AppDirError),
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> RevisionError + '_ {
    move |source| RevisionError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// One file of a revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub sha256: String,
    /// The folder under `plugins/<name>/` that holds the bytes.
    pub revision: String,
}

/// Every file of a revision, wherever its bytes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionIndex {
    pub name: String,
    pub version: String,
    /// The revision this one was compared with; none for a plugin's first.
    pub previous: Option<String>,
    /// Logical path (`plugin.toml`, `pages/notes.xml`, `plugin.wasm`) to its entry.
    pub files: BTreeMap<String, FileEntry>,
}

/// What [`store`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The new folder's name.
    pub revision: String,
    pub index: RevisionIndex,
    /// Files written into the new folder: new or changed.
    pub written: Vec<String>,
    /// Files left where they were because their hash did not change.
    pub reused: Vec<String>,
}

/// Where a file's bytes are, relative to the app_dir root.
pub fn relative_path(name: &str, entry: &FileEntry, logical: &str) -> PathBuf {
    Path::new("plugins").join(name).join(&entry.revision).join(logical)
}

/// The revision folder `folder` of plugin `name` as an index: from its `files.json`, or,
/// for a folder in the older layout, from hashing the files in it. `None` when there is no
/// such folder.
pub async fn read_index(
    layout: &AppDir,
    name: &str,
    folder: &str,
) -> Result<Option<RevisionIndex>, RevisionError> {
    let directory = layout.revision_dir(name, folder)?;
    let index_file = directory.join(INDEX_FILE);
    match tokio::fs::read_to_string(&index_file).await {
        Ok(text) => {
            return serde_json::from_str(&text)
                .map(Some)
                .map_err(|source| RevisionError::Index { path: index_file, source });
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&index_file)(error)),
    }
    if !directory.is_dir() {
        return Ok(None);
    }
    index_of_folder(name, folder, &directory).await.map(Some)
}

/// Hash every file under `directory` (an older-layout folder holding all of a version's files).
async fn index_of_folder(
    name: &str,
    folder: &str,
    directory: &Path,
) -> Result<RevisionIndex, RevisionError> {
    let mut files = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&current).await.map_err(io_error(&current))?;
        while let Some(entry) = entries.next_entry().await.map_err(io_error(&current))? {
            let path = entry.path();
            if entry.file_type().await.map_err(io_error(&path))?.is_dir() {
                pending.push(path);
                continue;
            }
            let bytes = tokio::fs::read(&path).await.map_err(io_error(&path))?;
            let logical = path
                .strip_prefix(directory)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            files.insert(
                logical,
                FileEntry {
                    sha256: sha256_hex(&bytes),
                    revision: folder.to_string(),
                },
            );
        }
    }
    Ok(RevisionIndex {
        name: name.to_string(),
        version: folder.to_string(),
        previous: None,
        files,
    })
}

/// Store a new revision of plugin `name`.
///
/// `files` is every file of the plugin as `(logical path, bytes)`. Each is compared by
/// hash with `previous` (the plugin's latest revision, if it has one): a file that is the
/// same is not copied, the index just points at the folder that already has it; a file
/// that is new or different is written into the new, time-stamped folder.
pub async fn store(
    layout: &AppDir,
    name: &str,
    version: &str,
    previous: Option<(&str, &RevisionIndex)>,
    files: &[(String, Vec<u8>)],
) -> Result<Stored, RevisionError> {
    let root = layout.plugin_root(name)?;
    tokio::fs::create_dir_all(&root).await.map_err(io_error(&root))?;
    let (revision, directory) = create_unique_folder(&root, &stamp_now()).await?;

    let mut index = RevisionIndex {
        name: name.to_string(),
        version: version.to_string(),
        previous: previous.map(|(folder, _)| folder.to_string()),
        files: BTreeMap::new(),
    };
    let (mut written, mut reused) = (Vec::new(), Vec::new());
    let result: Result<(), RevisionError> = async {
        for (logical, bytes) in files {
            let sha256 = sha256_hex(bytes);
            let unchanged = previous
                .and_then(|(_, index)| index.files.get(logical))
                .filter(|entry| entry.sha256 == sha256);
            if let Some(entry) = unchanged {
                index.files.insert(logical.clone(), entry.clone());
                reused.push(logical.clone());
                continue;
            }
            let target = directory.join(logical);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(io_error(parent))?;
            }
            tokio::fs::write(&target, bytes).await.map_err(io_error(&target))?;
            index.files.insert(
                logical.clone(),
                FileEntry {
                    sha256,
                    revision: revision.clone(),
                },
            );
            written.push(logical.clone());
        }
        let text = serde_json::to_string_pretty(&index)
            .map_err(|source| RevisionError::Index { path: directory.join(INDEX_FILE), source })?;
        let file = directory.join(INDEX_FILE);
        tokio::fs::write(&file, text).await.map_err(io_error(&file))
    }
    .await;
    if let Err(error) = result {
        // A half-written revision must not be mistaken for a complete one.
        let _ = tokio::fs::remove_dir_all(&directory).await;
        return Err(error);
    }
    Ok(Stored { revision, index, written, reused })
}

/// Files of `index` that are not on disk any more.
pub async fn missing_files(layout: &AppDir, index: &RevisionIndex) -> Result<Vec<String>, RevisionError> {
    let mut missing = Vec::new();
    for (logical, entry) in &index.files {
        let path = layout.revision_dir(&index.name, &entry.revision)?.join(logical);
        if !path.is_file() {
            missing.push(logical.clone());
        }
    }
    Ok(missing)
}

/// Write back `files` that `index` lists but that are gone from disk (for example after
/// `app_dir` was cleaned), at the places the index says they belong.
pub async fn restore(
    layout: &AppDir,
    index: &RevisionIndex,
    files: &[(String, Vec<u8>)],
) -> Result<(), RevisionError> {
    for (logical, bytes) in files {
        let Some(entry) = index.files.get(logical) else { continue };
        if entry.sha256 != sha256_hex(bytes) {
            continue;
        }
        let target = layout.revision_dir(&index.name, &entry.revision)?.join(logical);
        if target.is_file() {
            continue;
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(io_error(parent))?;
        }
        tokio::fs::write(&target, bytes).await.map_err(io_error(&target))?;
    }
    Ok(())
}

/// Create `<root>/<stamp>`, or `<stamp>-2`, `-3`, ... when two revisions land in one second.
async fn create_unique_folder(root: &Path, stamp: &str) -> Result<(String, PathBuf), RevisionError> {
    let mut attempt = 1u32;
    loop {
        let name = if attempt == 1 { stamp.to_string() } else { format!("{stamp}-{attempt}") };
        let path = root.join(&name);
        match tokio::fs::create_dir(&path).await {
            Ok(()) => return Ok((name, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => attempt += 1,
            Err(error) => return Err(io_error(&path)(error)),
        }
    }
}

/// Now, as `YYYYMMDDTHHMMSSZ` in UTC.
pub fn stamp_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    stamp_of(seconds)
}

/// `seconds` since the Unix epoch as `YYYYMMDDTHHMMSSZ` in UTC.
pub fn stamp_of(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hour, minute, second) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
        list.iter().map(|(path, text)| (path.to_string(), text.as_bytes().to_vec())).collect()
    }

    #[test]
    fn stamps_are_utc_dates_and_times() {
        assert_eq!(stamp_of(0), "19700101T000000Z");
        assert_eq!(stamp_of(951_782_400), "20000229T000000Z", "a leap day");
        assert_eq!(stamp_of(1_791_057_600), "20261003T200000Z");
        assert_eq!(stamp_of(1_791_057_600 + 3 * 3600 + 25 * 60 + 9), "20261003T232509Z");
    }

    #[tokio::test]
    async fn the_first_revision_stores_everything_and_a_change_stores_only_the_difference()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let layout = AppDir::new(directory.path());

        let first = store(
            &layout, "notes", "0.1.0", None,
            &files(&[("plugin.toml", "a"), ("pages/notes.xml", "page"), ("plugin.wasm", "wasm-1")]),
        ).await?;
        assert_eq!(first.written.len(), 3);
        assert!(first.reused.is_empty());
        assert_eq!(first.index.previous, None);

        // Only the wasm changes.
        let second = store(
            &layout, "notes", "0.1.0", Some((&first.revision, &first.index)),
            &files(&[("plugin.toml", "a"), ("pages/notes.xml", "page"), ("plugin.wasm", "wasm-2")]),
        ).await?;
        assert_eq!(second.written, ["plugin.wasm"]);
        let mut reused = second.reused.clone();
        reused.sort();
        assert_eq!(reused, ["pages/notes.xml", "plugin.toml"]);
        assert_eq!(second.index.previous.as_deref(), Some(first.revision.as_str()));

        // The new folder holds just what changed (and the index); nothing is duplicated.
        let folder = layout.revision_dir("notes", &second.revision)?;
        assert!(folder.join("plugin.wasm").is_file());
        assert!(!folder.join("plugin.toml").exists());
        assert!(!folder.join("pages").exists());
        assert!(folder.join(INDEX_FILE).is_file());

        // The index still describes a complete revision, pointing at the older folder.
        assert_eq!(second.index.files["plugin.toml"].revision, first.revision);
        assert_eq!(second.index.files["plugin.wasm"].revision, second.revision);
        let read_back = read_index(&layout, "notes", &second.revision).await?;
        assert_eq!(read_back.as_ref(), Some(&second.index));
        assert!(missing_files(&layout, &second.index).await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn two_revisions_in_one_second_get_distinct_folders() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let layout = AppDir::new(directory.path());
        let one = store(&layout, "p", "1", None, &files(&[("plugin.toml", "a")])).await?;
        let two = store(&layout, "p", "1", None, &files(&[("plugin.toml", "b")])).await?;
        assert_ne!(one.revision, two.revision);
        Ok(())
    }

    #[tokio::test]
    async fn a_folder_in_the_older_layout_is_a_revision_too() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let layout = AppDir::new(directory.path());
        let legacy = layout.revision_dir("old", "1.0.0")?;
        tokio::fs::create_dir_all(legacy.join("pages")).await?;
        tokio::fs::write(legacy.join("plugin.toml"), "t").await?;
        tokio::fs::write(legacy.join("pages/a.xml"), "p").await?;
        tokio::fs::write(legacy.join("plugin.wasm"), "w").await?;

        let previous = read_index(&layout, "old", "1.0.0").await?.ok_or("no index")?;
        assert_eq!(previous.files.len(), 3);
        assert_eq!(previous.files["pages/a.xml"].revision, "1.0.0");

        let next = store(
            &layout, "old", "1.1.0", Some(("1.0.0", &previous)),
            &files(&[("plugin.toml", "t2"), ("pages/a.xml", "p"), ("plugin.wasm", "w")]),
        ).await?;
        assert_eq!(next.written, ["plugin.toml"]);
        assert_eq!(next.index.files["plugin.wasm"].revision, "1.0.0");
        assert_eq!(read_index(&layout, "old", "nope").await?, None);
        Ok(())
    }

    #[tokio::test]
    async fn missing_files_are_found_and_restored_where_they_belong() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let layout = AppDir::new(directory.path());
        let list = files(&[("plugin.toml", "a"), ("plugin.wasm", "w")]);
        let stored = store(&layout, "p", "1", None, &list).await?;
        let wasm = layout.revision_dir("p", &stored.revision)?.join("plugin.wasm");
        tokio::fs::remove_file(&wasm).await?;
        assert_eq!(missing_files(&layout, &stored.index).await?, ["plugin.wasm"]);
        restore(&layout, &stored.index, &list).await?;
        assert!(wasm.is_file());
        assert!(missing_files(&layout, &stored.index).await?.is_empty());
        Ok(())
    }
}
