//! Terminal-scoped history. Hold the lock through the request to serialize turns.
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

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

impl Session {
    pub fn open(reset: bool) -> Result<Self> {
        Self::open_at(std::env::temp_dir().join("terminal-aichat").join(terminal_id()?), reset)
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
}
