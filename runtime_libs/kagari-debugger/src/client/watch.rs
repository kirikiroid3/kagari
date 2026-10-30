//! Watching a project's files for `--watch` / `watch`: a polling watcher, called by a front end
//! about every half second. Each poll walks the roots and fingerprints every file by its path,
//! length and modification time; a change is reported once the new fingerprint has held for one
//! poll, so a save or a checkout that writes many files triggers one rebuild.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Watches files under some roots.
pub struct Watcher {
    roots: Vec<PathBuf>,
    /// Directories not walked (a project's output directories).
    skip: Vec<PathBuf>,
    /// The fingerprint last reported (or the first).
    settled: u64,
    /// The fingerprint of the previous poll.
    previous: u64,
}

impl Watcher {
    /// Watches every file under `roots` except in `skip` and in hidden directories; what is there
    /// now is the starting point.
    pub fn new(roots: Vec<PathBuf>, skip: Vec<PathBuf>) -> Watcher {
        let mut watcher = Watcher {
            roots,
            skip,
            settled: 0,
            previous: 0,
        };
        watcher.settle();
        watcher
    }

    /// Whether the files changed since the last change reported (or since [`Watcher::new`] or
    /// [`Watcher::settle`]) and have stayed the same since the previous call.
    pub fn changed(&mut self) -> bool {
        let now = self.fingerprint();
        let stable = now == self.previous;
        self.previous = now;
        if stable && now != self.settled {
            self.settled = now;
            return true;
        }
        false
    }

    /// Takes the files as they are now as the starting point: a rebuild that was not
    /// triggered by [`Watcher::changed`] has seen them.
    pub fn settle(&mut self) {
        self.settled = self.fingerprint();
        self.previous = self.settled;
    }

    fn fingerprint(&self) -> u64 {
        let mut files = Vec::new();
        for root in &self.roots {
            self.walk(root, &mut files);
        }
        files.sort_unstable();
        let mut hasher = DefaultHasher::new();
        files.hash(&mut hasher);
        hasher.finish()
    }

    /// Collects `(path, length, modified)` of the files under `dir`.
    fn walk(&self, dir: &Path, files: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if !hidden && !self.skip.contains(&path) {
                    self.walk(&path, files);
                }
            } else if let Ok(meta) = entry.metadata() {
                files.push((path, meta.len(), meta.modified().ok()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn a_change_is_reported_once_it_holds_for_a_poll() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let out = root.join("target");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("src/main.kis"), "one").unwrap();
        let mut watcher = Watcher::new(vec![root.clone()], vec![out.clone()]);
        assert!(!watcher.changed());

        std::fs::write(root.join("src/main.kis"), "two!").unwrap();
        assert!(!watcher.changed(), "not before it held for one poll");
        std::fs::write(root.join("src/new.kis"), "three").unwrap();
        assert!(!watcher.changed(), "still changing");
        assert!(watcher.changed());
        assert!(!watcher.changed(), "reported once");

        // Output and hidden directories are not watched.
        std::fs::write(out.join("game.kagari"), "bytes").unwrap();
        std::fs::write(root.join(".git/index"), "bytes").unwrap();
        assert!(!watcher.changed());
        assert!(!watcher.changed());

        // A rebuild by hand settles a change.
        std::fs::remove_file(root.join("src/new.kis")).unwrap();
        watcher.settle();
        assert!(!watcher.changed());
        assert!(!watcher.changed());
    }
}
