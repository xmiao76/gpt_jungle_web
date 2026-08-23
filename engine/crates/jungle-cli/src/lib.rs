pub mod arena;
pub mod cli;
pub mod corpus;
pub mod protocol;
pub mod server;
pub mod stats;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    cli::run_from_env()
}
