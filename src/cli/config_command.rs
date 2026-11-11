use crate::config::{ConfigManager, encrypt_api_key, print_config_location, print_prompts, print_providers};
use clap::{Parser, Subcommand};
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "aichat config",
    bin_name = "aichat config",
    about = "Manage configuration and provider credentials"
)]
struct ConfigCli {
    /// Config file path (TERMINAL_AICHT_CONFIG overrides this value)
    #[arg(long, global = true)]
    config: Option<String>,
    #[command(subcommand)]
    command: ConfigCommand,
}
#[derive(Subcommand)]
enum ConfigCommand {
    /// Create default configuration; warn without overwriting an existing file
    Init,
    /// Show configured providers, models, prompts and config location
    Show,
    /// Manage providers
    Provider {
        #[command(subcommand)]
        command: ListCommand,
    },
    /// Manage models
    Model {
        #[command(subcommand)]
        command: ListCommand,
    },
    /// Enter, confirm and encrypt a provider API key
    Login {
        /// Existing provider ID (TERMINAL_AICHT_PROVIDER overrides this value)
        #[arg(long)]
        provider: Option<String>,
    },
}
#[derive(Subcommand)]
enum ListCommand {
    List,
}

fn environment(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) => Err(format!("{name}: value must not be empty")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(_) => Err(format!("{name}: invalid Unicode")),
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = ConfigCli::parse_from(std::env::args_os().skip(1));
    let path = environment("TERMINAL_AICHT_CONFIG")?.or(cli.config);
    let manager = ConfigManager::new(&ConfigManager::get_config_dir()?, path.as_deref())?;
    if matches!(cli.command, ConfigCommand::Init) {
        if manager.init_default()? {
            eprintln!("Created default config at {}", manager.get_config_path().display());
        } else {
            eprintln!(
                "WARN: Config already exists at {}; leaving it unchanged.",
                manager.get_config_path().display()
            );
        }
        return Ok(());
    }
    if !manager.exists() {
        return Err("Config file does not exist; run aichat config init first".into());
    }
    let config = manager.load()?;
    match cli.command {
        ConfigCommand::Init => unreachable!(),
        ConfigCommand::Show => {
            print_providers(&config)?;
            print_prompts(&config);
            println!();
            print_config_location(manager.get_config_path());
        }
        ConfigCommand::Provider { .. } => {
            let mut providers: Vec<_> = config.providers.iter().collect();
            providers.sort_by_key(|(id, _)| *id);
            for (id, provider) in providers {
                println!(
                    "{} ({}){}",
                    id,
                    provider.get_name(id),
                    if config.default_provider.as_deref() == Some(id) {
                        " *"
                    } else {
                        ""
                    }
                );
            }
        }
        ConfigCommand::Model { .. } => {
            let mut models: Vec<_> = config
                .providers
                .iter()
                .flat_map(|(id, provider)| {
                    provider
                        .models
                        .iter()
                        .map(move |model| (format!("{id}/{}", model.id), model.get_name()))
                })
                .collect();
            models.sort();
            for (selector, name) in models {
                println!("{selector} ({name})");
            }
        }
        ConfigCommand::Login { provider } => {
            let provider = environment("TERMINAL_AICHT_PROVIDER")?
                .or(provider)
                .ok_or("Specify --provider <ID> (or TERMINAL_AICHT_PROVIDER)")?;
            if !config.providers.contains_key(&provider) {
                return Err(format!("Provider '{provider}' not found; configure it before logging in").into());
            }
            let value = read_secret(&format!("API key for {provider}: "))?;
            if value.is_empty() {
                return Err("API key must not be empty".into());
            }
            let confirmation = read_secret("Confirm API key: ")?;
            if value != confirmation {
                return Err("API keys do not match; configuration unchanged".into());
            }
            let encrypted = encrypt_api_key(&value)?;
            manager.save_api_key(&provider, &encrypted)?;
            eprintln!("Saved encrypted API key for {provider}.");
        }
    }
    Ok(())
}

// Open the controlling terminal directly: login must never consume piped chat input.
#[cfg(unix)]
fn read_secret(prompt: &str) -> io::Result<String> {
    use std::{fs::OpenOptions, io::Read, os::fd::AsRawFd};
    let mut terminal = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| io::Error::other("Login requires an interactive terminal"))?;
    let fd = terminal.as_raw_fd();
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let original = unsafe { original.assume_init() };
    let mut hidden = original;
    // Read without echo; handle cancellation ourselves so terminal state is restored.
    hidden.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ISIG | libc::ICANON);
    hidden.c_cc[libc::VMIN] = 1;
    hidden.c_cc[libc::VTIME] = 0;
    if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &hidden) } != 0 {
        return Err(io::Error::last_os_error());
    }
    struct Restore(i32, libc::termios);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                libc::tcsetattr(self.0, libc::TCSANOW, &self.1);
            }
        }
    }
    let restore = Restore(fd, original);
    write!(terminal, "{prompt}")?;
    terminal.flush()?;
    let mut value = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        if terminal.read(&mut byte)? == 0 || matches!(byte[0], 3 | 4) {
            drop(restore);
            writeln!(terminal)?;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Login cancelled; configuration unchanged",
            ));
        }
        match byte[0] {
            b'\r' | b'\n' => break,
            8 | 127 => while value.pop().is_some_and(|b| b & 0xc0 == 0x80) {},
            0..=31 => {}
            byte => value.push(byte),
        }
    }
    drop(restore);
    writeln!(terminal)?;
    String::from_utf8(value).map_err(|_| io::Error::other("API key must be valid Unicode"))
}

#[cfg(not(unix))]
fn read_secret(prompt: &str) -> io::Result<String> {
    use crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        terminal,
    };
    use std::io::IsTerminal;
    if !io::stdin().is_terminal() {
        return Err(io::Error::other("Login requires an interactive terminal"));
    }
    terminal::enable_raw_mode()?;
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = terminal::disable_raw_mode();
        }
    }
    let restore = Restore;
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut value = String::new();
    loop {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => break,
                KeyCode::Esc => return Err(io::Error::new(io::ErrorKind::Interrupted, "Login cancelled")),
                KeyCode::Backspace => {
                    value.pop();
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "Login cancelled"));
                }
                KeyCode::Char(c) => value.push(c),
                _ => {}
            }
        }
    }
    drop(restore);
    eprintln!();
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolated_config_arguments() {
        for args in [
            vec!["config", "init"],
            vec!["config", "show"],
            vec!["config", "provider", "list"],
            vec!["config", "model", "list"],
            vec!["config", "login", "--provider", "openrouter"],
        ] {
            assert!(ConfigCli::try_parse_from(args).is_ok());
        }
        assert!(ConfigCli::try_parse_from(["config", "show", "--model", "x"]).is_err());
        assert!(ConfigCli::try_parse_from(["config", "show", "--display=usage"]).is_err());
        assert!(ConfigCli::try_parse_from(["config", "--config", "custom.jsonc", "model", "list"]).is_ok());
        assert!(ConfigCli::try_parse_from(["config", "model", "list", "--config", "custom.jsonc"]).is_ok());
        let chat = crate::cli::structs::Cli::try_parse_from(["aichat", "--model", "x", "config", "show"]).unwrap();
        assert_eq!(chat.input, ["config", "show"]);
        let chat = crate::cli::structs::Cli::try_parse_from(["aichat", "--", "config", "show"]).unwrap();
        assert_eq!(chat.input, ["config", "show"]);
    }
}
