// logger.rs
use chrono::Local;
use crossterm::style::Stylize;
use std::sync::atomic::{AtomicU8, Ordering};
use std::{
    fmt,
    io::{self, Write},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl LogLevel {
    fn color(&self) -> &'static str {
        match self {
            LogLevel::Error => "\x1b[31m", // 红色
            LogLevel::Warn => "\x1b[33m",  // 黄色
            LogLevel::Info => "\x1b[32m",  // 绿色
            LogLevel::Debug => "\x1b[36m", // 青色
            LogLevel::Trace => "\x1b[37m", // 白色
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN ",
            LogLevel::Info => "INFO ",
            LogLevel::Debug => "DEBUG",
            LogLevel::Trace => "TRACE",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_uppercase().as_str() {
            "ERROR" => Ok(Self::Error),
            "WARN" => Ok(Self::Warn),
            "INFO" => Ok(Self::Info),
            "DEBUG" => Ok(Self::Debug),
            "TRACE" => Ok(Self::Trace),
            _ => Err("log-level: expected ERROR, WARN, INFO, DEBUG or TRACE".into()),
        }
    }
}

static CURRENT_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

pub fn init_logger() {
    let level = LogLevel::Info;
    CURRENT_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn set_log_level(level: LogLevel) {
    CURRENT_LEVEL.store(level as u8, Ordering::Relaxed);
}

pub fn log_impl(level: LogLevel, args: fmt::Arguments) {
    let current_level = CURRENT_LEVEL.load(Ordering::Relaxed);
    if (level as u8) > current_level {
        return;
    }

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let msg = format!(
        "{}[{}]\x1b[0m {} {}",
        level.color(),
        level.as_str(),
        timestamp.to_string().blue(),
        args
    );

    let _ = writeln!(io::stderr(), "{}", msg);
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_impl($crate::utils::logger::LogLevel::Error, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_impl($crate::utils::logger::LogLevel::Warn, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_impl($crate::utils::logger::LogLevel::Info, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_impl($crate::utils::logger::LogLevel::Debug, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::utils::logger::log_impl($crate::utils::logger::LogLevel::Trace, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger() {
        init_logger(); // 从环境变量初始化
        // 或者手动设置: set_log_level(LogLevel::Debug);

        let null_str: Result<String, ()> = Result::Err(());
        assert_eq!(null_str.unwrap_or_default(), ""); //字符串default是空

        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Debug.color(), "\x1b[36m");

        let name = "Rust";
        let version = 1.75;

        log_error!("Error: {} version {}", name, version);
        log_warn!("Warning: {} version {}", name, version);
        log_info!("Info: {} version {}", name, version);
        log_debug!("Debug: {} version {}", name, version);
        log_trace!("Trace: {} version {}", name, version);
    }
}
