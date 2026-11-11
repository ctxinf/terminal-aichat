use tokio::runtime::Runtime;

use crate::utils::logger::init_logger;

mod chat;
mod cli;
mod config;
mod integrations;
mod prompt;
mod session;
mod utils;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();
    let rt = Runtime::new().unwrap();
    rt.block_on(cli::cli::main())
}
