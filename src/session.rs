//! Terminal-scoped history. Hold the lock through the request to serialize turns.
use crate::config::SessionsConfig;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const CLEANUP_INTERVAL: Duration = Duration::from_secs(86_400);
const DAY_SECS: u64 = 86_400;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Session {
    pub history: Vec<Value>,
    path: PathBuf,
    _lock: File,
}

fn terminal_id() -> Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // /dev/tty works even when stdin/stdout are pipes or command substitution.
        let tty = OpenOptions::new()
            .read(true)
            .open("/dev/tty")
            .map_err(|_| "-c requires a controlling terminal")?;
        let mut name = vec![0u8; 1024];
        use std::os::fd::AsRawFd;
        // SAFETY: the descriptor and writable buffer are valid.
        let result = unsafe { libc::ttyname_r(tty.as_raw_fd(), name.as_mut_ptr().cast(), name.len()) };
        if result != 0 {
            return Err("cannot identify current terminal".into());
        }
        let end = name.iter().position(|b| *b == 0).ok_or("invalid terminal name")?;
        let path = std::str::from_utf8(&name[..end])?;
        let metadata = fs::metadata(path)?;
        // Include login session to avoid reusing history when a PTY number is recycled.
        let sid = unsafe { libc::getsid(0) };
        #[cfg(not(target_os = "linux"))]
        let device = metadata.rdev();
        #[cfg(target_os = "linux")]
        let device = {
            let mut actual: libc::c_uint = 0;
            // SAFETY: TIOCGDEV writes a device number to a valid unsigned int.
            if unsafe { libc::ioctl(tty.as_raw_fd(), libc::TIOCGDEV, &mut actual) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            actual as u64
        };
        Ok(format!("{}-{}-{}", metadata.uid(), device, sid))
    }
    #[cfg(not(unix))]
    {
        Err("terminal sessions are currently supported on Unix only".into())
    }
}

fn cleanup_marker_path(base_dir: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        base_dir.join(format!(".cleanup-{}", unsafe { libc::geteuid() }))
    }
    #[cfg(not(unix))]
    {
        base_dir.join(".cleanup")
    }
}

fn touch_cleanup_marker(marker_path: &Path, now: SystemTime) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(marker_path)?;
    file.set_modified(now)?;
    Ok(())
}

fn cleanup_terminal_dir(term_dir: &Path, max_age: Duration, now: SystemTime) {
    // Capture mtime before deleting files inside `term_dir`, since unlinking updates directory mtime.
    let dir_expired = fs::symlink_metadata(term_dir)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|mtime| now.duration_since(mtime).ok())
        .is_some_and(|age| age > max_age);
    let Ok(entries) = fs::read_dir(term_dir) else {
        return;
    };
    let mut has_remaining_jsonl = false;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let is_jsonl = path.extension().and_then(|e| e.to_str()) == Some("jsonl");
        if !is_jsonl {
            continue;
        }
        let expired = meta
            .modified()
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > max_age);
        if expired {
            let _ = fs::remove_file(&path);
        } else {
            has_remaining_jsonl = true;
        }
    }
    if has_remaining_jsonl || !dir_expired {
        return;
    }
    #[cfg(unix)]
    {
        let lock_path = term_dir.join("lock");
        if let Ok(lock_meta) = fs::symlink_metadata(&lock_path) {
            if lock_meta.is_file() {
                use std::os::fd::AsRawFd;
                use std::os::unix::fs::OpenOptionsExt;
                let Ok(lock_file) = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&lock_path)
                else {
                    return;
                };
                if unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                    return;
                }
            }
        }
    }
    let _ = fs::remove_dir_all(term_dir);
}

fn cleanup_expired_at(
    base_dir: &Path,
    auto_delete_days: u64,
    check_interval: Duration,
    now: SystemTime,
) -> Result<()> {
    if !base_dir.is_dir() {
        return Ok(());
    }
    let marker_path = cleanup_marker_path(base_dir);
    if let Ok(meta) = fs::symlink_metadata(&marker_path) {
        if !meta.is_file() {
            return Ok(());
        }
        if let Ok(mtime) = meta.modified() {
            if now.duration_since(mtime).is_ok_and(|elapsed| elapsed <= check_interval) {
                return Ok(());
            }
        }
    }
    touch_cleanup_marker(&marker_path, now)?;
    let max_age = Duration::from_secs(auto_delete_days.saturating_mul(DAY_SECS));
    for entry in fs::read_dir(base_dir)?.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.uid() != unsafe { libc::geteuid() } {
                continue;
            }
        }
        cleanup_terminal_dir(&path, max_age, now);
    }
    Ok(())
}

impl Session {
    pub fn open(reset: bool, sessions: &SessionsConfig) -> Result<Self> {
        let base_dir = std::env::temp_dir().join("terminal-aichat");
        if sessions.auto_delete {
            let _ = cleanup_expired_at(&base_dir, sessions.auto_delete_days, CLEANUP_INTERVAL, SystemTime::now());
        }
        Self::open_at(base_dir.join(terminal_id()?), reset)
    }

    fn open_at(directory: PathBuf, reset: bool) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, MetadataExt};
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true).mode(0o700).create(&directory)?;
            let metadata = fs::symlink_metadata(&directory)?;
            if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
                return Err("session directory must be private and owned by the current user".into());
            }
        }
        #[cfg(not(unix))]
        fs::create_dir_all(&directory)?;
        let mut options = OpenOptions::new();
        options.create(true).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let lock = options.open(directory.join("lock"))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let current = directory.join("current");
        let id = if reset || !current.exists() {
            let id = format!(
                "{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos(),
                std::process::id()
            );
            fs::write(&current, &id)?;
            id
        } else {
            fs::read_to_string(&current)?
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return Err("invalid session ID".into());
        }
        let path = directory.join(format!("{id}.jsonl"));
        let file = options.open(&path)?;
        let mut history = Vec::new();
        for line in BufReader::new(file).lines() {
            let value: Value = serde_json::from_str(&line?)?;
            if !matches!(value["role"].as_str(), Some("user" | "assistant")) || !value["content"].is_string() {
                return Err("invalid session history message".into());
            }
            history.push(value);
        }
        Ok(Self {
            history,
            path,
            _lock: lock,
        })
    }

    pub fn save(&self, user: Value, assistant: Value) -> Result<()> {
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        let turn = format!(
            "{}\n{}\n",
            serde_json::to_string(&user)?,
            serde_json::to_string(&assistant)?
        );
        file.write_all(turn.as_bytes())?;
        file.sync_data()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn history_and_rotation() {
        let dir = std::env::temp_dir().join(format!("aichat-session-test-{}", std::process::id()));
        let first = Session::open_at(dir.clone(), false).unwrap();
        assert!(first.history.is_empty());
        first
            .save(
                json!({"role":"user","content":"hello\nworld"}),
                json!({"role":"assistant","content":"answer"}),
            )
            .unwrap();
        let old = first.path.clone();
        drop(first);
        let continued = Session::open_at(dir.clone(), false).unwrap();
        assert_eq!(continued.history.len(), 2);
        drop(continued);
        let fresh = Session::open_at(dir.clone(), true).unwrap();
        assert!(fresh.history.is_empty());
        assert_ne!(fresh.path, old);
        assert!(old.exists());
        drop(fresh);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_respects_marker_interval_and_age_threshold() {
        let base = std::env::temp_dir().join(format!("aichat-cleanup-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let term_mixed = base.join("term-mixed");
        let term_stale = base.join("term-stale");
        fs::create_dir_all(&term_mixed).unwrap();
        fs::create_dir_all(&term_stale).unwrap();

        let now = SystemTime::now();
        let eight_days_ago = now - Duration::from_secs(8 * DAY_SECS);
        let six_days_ago = now - Duration::from_secs(6 * DAY_SECS);
        let half_day_ago = now - Duration::from_secs(DAY_SECS / 2);
        let two_days_ago = now - Duration::from_secs(2 * DAY_SECS);

        let expired_file = term_mixed.join("1-1.jsonl");
        let recent_file = term_mixed.join("2-1.jsonl");
        let stale_only_file = term_stale.join("3-1.jsonl");

        for (path, mtime) in [
            (&expired_file, eight_days_ago),
            (&recent_file, six_days_ago),
            (&stale_only_file, eight_days_ago),
        ] {
            let file = File::create(path).unwrap();
            file.set_modified(mtime).unwrap();
        }
        // Make term_stale directory itself older than 7 days so it gets pruned once empty.
        File::open(&term_stale).unwrap().set_modified(eight_days_ago).unwrap();

        // Recent marker (12h ago <= 1 day): cleanup must be skipped.
        let marker = cleanup_marker_path(&base);
        touch_cleanup_marker(&marker, half_day_ago).unwrap();
        cleanup_expired_at(&base, 7, CLEANUP_INTERVAL, now).unwrap();
        assert!(expired_file.exists());
        assert!(recent_file.exists());
        assert!(stale_only_file.exists());

        // Stale marker (2d ago > 1 day): cleanup runs, deletes >7d files and empty stale dir.
        touch_cleanup_marker(&marker, two_days_ago).unwrap();
        cleanup_expired_at(&base, 7, CLEANUP_INTERVAL, now).unwrap();
        assert!(!expired_file.exists());
        assert!(recent_file.exists());
        assert!(!stale_only_file.exists());
        assert!(!term_stale.exists());
        assert!(term_mixed.exists());

        fs::remove_dir_all(base).unwrap();
    }
}
