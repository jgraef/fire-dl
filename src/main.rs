mod args;
mod download;
mod scan;
mod utils;

pub use color_eyre::eyre::Error;
use structopt::StructOpt;

use crate::args::Args;

pub const DEFAULT_USER_AGENT: &'static str = "fire-dl";

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let args = Args::from_args();
    args.run().await?;

    Ok(())
}
