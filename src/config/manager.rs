use super::Config;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 配置文件管理器 - 只负责I/O操作
pub struct ConfigManager {
    config_path: PathBuf,
    is_default_path: bool,
}

impl ConfigManager {
    /// 获取跨平台的配置目录
    /// - Windows: %APPDATA%\terminal-aichat
    /// - macOS: ~/Library/Application Support/terminal-aichat
    /// - Linux: ~/.config/terminal-aichat
    pub fn get_config_dir() -> io::Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Cannot obtain config directory"))?;
        Ok(config_dir.join("terminal-aichat"))
    }

    pub fn new(config_dir: &Path, custom_path: Option<&str>) -> io::Result<Self> {
        if !config_dir.exists() {
            fs::create_dir_all(config_dir)?;
        }

        let (config_path, is_default_path) = if let Some(path) = custom_path {
            (PathBuf::from(path), false)
        } else {
            // 先尝试 .jsonc，再尝试 .json，默认使用 .jsonc
            let jsonc_path = config_dir.join("config.jsonc");
            let json_path = config_dir.join("config.json");
            if jsonc_path.exists() {
                (jsonc_path, true)
            } else if json_path.exists() {
                (json_path, true)
            } else {
                // 默认使用 .jsonc
                (jsonc_path, true)
            }
        };

        Ok(Self {
            config_path,
            is_default_path,
        })
    }

    /// 是否使用默认配置路径
    pub fn is_default_path(&self) -> bool {
        self.is_default_path
    }

    /// 从文件加载配置
    pub fn load(&self) -> io::Result<Config> {
        if !self.config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&self.config_path)?;

        // 使用 serde_json_lenient 解析（支持 JSONC 注释）
        let mut deserializer = serde_json_lenient::Deserializer::from_str(&content);
        let config: Config = serde_path_to_error::deserialize(&mut deserializer)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        deserializer
            .end()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        config
            .validate()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(config)
    }

    /// 保存默认配置（带注释）到文件
    pub fn save_default_with_comments(&self) -> io::Result<()> {
        fs::write(&self.config_path, Config::default_config_with_comments())
    }

    /// Atomically create a default configuration; return false if it already exists.
    pub fn init_default(&self) -> io::Result<bool> {
        use std::io::Write;
        if let Some(parent) = self.config_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.config_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(error) => return Err(error),
        };
        file.write_all(Config::default_config_with_comments().as_bytes())?;
        Ok(true)
    }

    /// Save only the selected provider credential, preserving JSONC comments.
    pub fn save_api_key(&self, provider: &str, key: &super::ApiKeyConfig) -> io::Result<()> {
        use std::io::Write;
        let original = fs::read_to_string(&self.config_path)?;
        let updated = super::edit::replace_api_key(&original, provider, key).map_err(io::Error::other)?;
        let config: Config =
            serde_json_lenient::from_str(&updated).map_err(|_| io::Error::other("Updated config is invalid"))?;
        config.validate().map_err(io::Error::other)?;
        let parent = self
            .config_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temporary = parent.join(format!(".aichat-login-{}.tmp", std::process::id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let result = (|| {
            file.write_all(updated.as_bytes())?;
            file.sync_all()?;
            drop(file);
            if fs::read_to_string(&self.config_path)? != original {
                return Err(io::Error::other("Config changed during login; retry"));
            }
            fs::rename(&temporary, &self.config_path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    /// 配置文件是否存在
    pub fn exists(&self) -> bool {
        self.config_path.exists()
    }

    /// 获取配置文件路径
    pub fn get_config_path(&self) -> &Path {
        &self.config_path
    }
}
