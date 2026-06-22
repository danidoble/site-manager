//! Backups (specs §Backups).
//!
//! Creates a tar.gz of: the SQLite DB, config.toml, the CA, the certificates
//! directory, and the generated nginx configs. Restore extracts into a target dir.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::domain::BackupEntry;
use crate::error::Result;

/// Create a backup archive in `backups_dir` from the given storage root.
///
/// `name_stamp` is a caller-supplied timestamp string used in the filename.
pub fn create_backup(backups_dir: &Path, storage_root: &Path, name_stamp: &str) -> Result<BackupEntry> {
    fs::create_dir_all(backups_dir)?;
    let archive_name = format!("backup-{name_stamp}.tar.gz");
    let archive_path = backups_dir.join(&archive_name);

    let tar_gz = fs::File::create(&archive_path)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let entries: &[&str] = &[
        "database.sqlite",
        "config.toml",
        "ca",
        "certificates",
        "nginx-generated",
    ];
    for name in entries {
        let p = storage_root.join(name);
        if p.exists() {
            if p.is_dir() {
                tar.append_dir_all(name, &p)?;
            } else {
                tar.append_file(name, &mut fs::File::open(&p)?)?;
            }
        }
    }
    let mut enc = tar.into_inner()?;
    enc.try_finish()?;
    flush(&mut enc)?; // ensure metadata flush where possible

    let meta = fs::metadata(&archive_path)?;
    Ok(BackupEntry {
        id: archive_name.clone(),
        name: archive_name,
        path: archive_path.to_string_lossy().to_string(),
        size_bytes: meta.len(),
        created_at: name_stamp.to_string(),
    })
}

fn flush(enc: &mut GzEncoder<fs::File>) -> Result<()> {
    // GzEncoder::get_ref -> &File; flush is best-effort.
    let _ = enc.get_ref().flush();
    Ok(())
}

/// List backup archives in a directory, newest first.
pub fn list_backups(backups_dir: &Path) -> Result<Vec<BackupEntry>> {
    let mut out = Vec::new();
    if !backups_dir.exists() {
        return Ok(out);
    }
    for e in fs::read_dir(backups_dir)? {
        let e = e?;
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) == Some("gz") {
            let name = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata()?;
            let created = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_default();
            out.push(BackupEntry {
                id: name.clone(),
                name,
                path: path.to_string_lossy().to_string(),
                size_bytes: meta.len(),
                created_at: created,
            });
        }
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

pub fn delete_backup(backups_dir: &Path, name: &str) -> Result<()> {
    let path = backups_dir.join(name);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Restore a backup archive into `dest` (extracts the tarball there).
pub fn restore_backup(archive_path: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dest)?;
    let tar_gz = fs::File::open(archive_path)?;
    let dec = flate2::read::GzDecoder::new(tar_gz);
    let mut tar = tar::Archive::new(dec);
    let mut extracted = Vec::new();
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        entry.unpack_in(dest)?;
        extracted.push(path);
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("store");
        let backups = tmp.path().join("backups");
        fs::create_dir_all(root.join("ca")).unwrap();
        fs::write(root.join("config.toml"), "dry_run = false").unwrap();
        fs::write(root.join("database.sqlite"), "SQLITE").unwrap();

        let entry = create_backup(&backups, &root, "20260101T000000Z").unwrap();
        assert!(entry.path.ends_with(".tar.gz"));
        assert!(entry.size_bytes > 0);

        let list = list_backups(&backups).unwrap();
        assert_eq!(list.len(), 1);

        let dest = tmp.path().join("restore");
        let files = restore_backup(Path::new(&entry.path), &dest).unwrap();
        assert!(files.iter().any(|p| p.to_string_lossy().contains("config.toml")));
        assert_eq!(
            fs::read_to_string(dest.join("config.toml")).unwrap(),
            "dry_run = false"
        );
    }
}
