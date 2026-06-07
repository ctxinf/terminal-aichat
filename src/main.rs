use tokio::runtime::Runtime;

use crate::utils::logger::init_logger;

mod config;
mod cli;
mod chat;
mod utils;
mod integrations;

fn main()->Result<(),Box<dyn std::error::Error>>{
    init_logger();
    let rt = Runtime::new().unwrap();
    rt.block_on(cli::cli::main())
}