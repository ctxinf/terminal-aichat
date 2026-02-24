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

        Ok(Self { config_path, is_default_path })
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
        let config: Config = serde_json_lenient::from_str(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(config)
    }


    /// 保存默认配置（带注释）到文件
    pub fn save_default_with_comments(&self) -> io::Result<()> {
        fs::write(&self.config_path, Config::default_config_with_comments())
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
