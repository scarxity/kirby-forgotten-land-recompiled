//! "Back up saves now": the save folder copied into save-backups, the newest few kept.
use std::fs;
use std::path::{Path, PathBuf};

const KEPT: usize = 5;

/// `save-YYYYMMDD-HHMMSS`: only folders named so are ever removed from the backups folder.
fn is_backup(name: &str) -> bool {
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    name.strip_prefix("save-").and_then(|r| r.split_once('-')).is_some_and(|(d, t)| digits(d, 8) && digits(t, 6))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let target = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// The new backup's folder. `stamp` is the time as YYYYMMDD-HHMMSS.
pub fn back_up(save: &Path, backups: &Path, stamp: &str) -> Result<PathBuf, String> {
    if fs::read_dir(save).map_or(true, |mut d| d.next().is_none()) {
        return Err("There are no saves yet.".into());
    }
    let dest = backups.join(format!("save-{stamp}"));
    copy_dir(save, &dest).map_err(|e| format!("The backup failed: {e}"))?;
    let mut all: Vec<String> = fs::read_dir(backups).into_iter().flatten().flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| is_backup(n)).collect();
    all.sort();
    for old in &all[..all.len().saturating_sub(KEPT)] {
        let _ = fs::remove_dir_all(backups.join(old));
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kfl-saves-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_backup_holds_the_whole_save_folder() {
        let root = scratch("whole");
        fs::create_dir_all(root.join("save").join("save_1")).unwrap();
        fs::write(root.join("save").join("save_1").join("slot.bin"), b"data").unwrap();
        let dest = back_up(&root.join("save"), &root.join("save-backups"), "20261119-120000").unwrap();
        assert_eq!(fs::read(dest.join("save_1").join("slot.bin")).unwrap(), b"data");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_saves_is_said_not_backed_up_empty() {
        let root = scratch("none");
        assert!(back_up(&root.join("save"), &root.join("save-backups"), "20261119-120000").is_err());
        fs::create_dir_all(root.join("save")).unwrap();
        assert!(back_up(&root.join("save"), &root.join("save-backups"), "20261119-120000").is_err());
        assert!(!root.join("save-backups").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_the_newest_five_are_kept_and_other_folders_stay() {
        let root = scratch("keep");
        fs::create_dir_all(root.join("save")).unwrap();
        fs::write(root.join("save").join("a.bin"), b"a").unwrap();
        let backups = root.join("save-backups");
        fs::create_dir_all(backups.join("mine")).unwrap();
        for i in 0..7 {
            back_up(&root.join("save"), &backups, &format!("20261119-12000{i}")).unwrap();
        }
        let mut left: Vec<String> = fs::read_dir(&backups).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["mine", "save-20261119-120002", "save-20261119-120003", "save-20261119-120004", "save-20261119-120005", "save-20261119-120006"]);
        fs::remove_dir_all(&root).unwrap();
    }
}
