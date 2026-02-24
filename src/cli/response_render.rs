use crossterm::
    style::Stylize
;
use std::{
    io::{Write, stdout},
    path::is_separator,
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    time::sleep,
};

use crate::{log_debug, log_trace};

/// 渲染配置
#[derive(Clone)]
pub struct RenderConfig {
    pub pure: bool,
    pub model_name: String,
    pub prompt_config_name: String,
    /// 打字机效果的速度（字符/秒）
    pub type_speed: u32,
    pub disable_stream: bool,
}

/// 响应渲染器
pub struct ResponseRenderer {
    /// 开始时间
    start_time: Instant,
}

impl ResponseRenderer {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }

    pub fn start_render(&self, config: RenderConfig) -> (Sender<String>, tokio::task::JoinHandle<()>) {
        let (message_tx, message_rx) = mpsc::channel(100);

        // 仅启动一个渲染任务，处理所有消息
        let render_handler = tokio::spawn(async move {
            Self::render_task_impl(config, message_rx).await;
        });
        return (message_tx, render_handler);
    }

    /// 渲染任务 - 处理所有消息并渲染
    async fn render_task_impl(config: RenderConfig, mut message_rx: Receiver<String>) {
        let mut stdout = stdout();
        if !config.pure {
            let _ = Self::render_status_bar(&config);
            log_trace!("Render Status Bar.");
        }

        let char_interval: Duration = Duration::from_secs_f32(1.0 / config.type_speed as f32);

        // 非阻塞处理所有待处理消息
        while let Some(value) = message_rx.recv().await {
            //渲染字符
            if config.disable_stream {
                print!("{}", value);
            } else {
                Self::print_with_interval(value.as_str(), char_interval).await;
            }
        }

        log_debug!("Message Receiver Exit.");
        // 结束时换行
        let _ = stdout.flush();
    }

    /// 渲染状态栏（固定在status_row）
    fn render_status_bar(config: &RenderConfig) {
        println!(
            "{}  {}: {}    {}: {}    {}",
            " > ".on_green(),
            "model".dark_green().bold(),
            config.model_name.as_str().cyan().bold(),
            "prompt".dark_green().bold(),
            config.prompt_config_name.as_str().blue().bold(),
            "".on_dark_green()
        );
    }
    pub fn render_tail_bar(&self) {
        let cost = Instant::now() - self.start_time;
        println!("\n✅{}\n", format!("{:#?}", cost).dark_green());
    }
    /// 异步函数：按给定时间间隔打印字符串的每个单词
    async fn print_with_interval(s: &str, word_interval: Duration) {
        let mut current_word = String::new();

        for c in s.chars() {
            current_word.push(c);

            // 遇到空格或换行时，打印当前单词并flush
            if c.is_whitespace() || c.is_ascii_punctuation() || is_separator(c) {
                print!("{}", current_word);
                use std::io::Write;
                std::io::stdout().flush().unwrap();

                current_word.clear();
                sleep(word_interval).await;
            }
        }

        // 处理最后一个单词（如果字符串不以空白字符结尾）
        if !current_word.is_empty() {
            print!("{}", current_word);
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        }
    }
}
