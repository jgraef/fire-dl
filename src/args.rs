use std::path::PathBuf;

use regex::Regex;
use structopt::StructOpt;
use tokio::{
    fs::File,
    io::{
        AsyncBufReadExt,
        BufReader,
    },
};
use url::Url;

use crate::{
    download::download,
    scan::scan,
    Error,
    DEFAULT_USER_AGENT,
};

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(flatten)]
    pub global_args: GlobalArgs,

    #[structopt(subcommand)]
    pub command: Command,
}

impl Args {
    pub async fn run(self) -> Result<(), Error> {
        match self.command {
            Command::Download(args) => download(self.global_args, args).await?,
            Command::Scan(args) => scan(self.global_args, args).await?,
        }

        Ok(())
    }
}

#[derive(Debug, StructOpt)]
pub struct GlobalArgs {
    #[structopt(long, default_value = DEFAULT_USER_AGENT)]
    pub user_agent: String,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(alias = "d")]
    Download(DownloadArgs),
    Scan(ScanArgs),
}

#[derive(Debug, StructOpt)]
pub struct DownloadArgs {
    #[structopt(short, long)]
    pub output: Option<PathBuf>,

    #[structopt(long)]
    pub redownload_existing: bool,

    #[structopt(short, long, default_value = "1")]
    pub parallel: usize,

    #[structopt(flatten)]
    pub urls: Urls,
}

#[derive(Debug, StructOpt)]
pub struct ScanArgs {
    #[structopt(short, long)]
    pub output: Option<PathBuf>,

    #[structopt(short, long)]
    pub recursive: bool,

    #[structopt(short, long)]
    pub no_parent: bool,

    #[structopt(short, long)]
    pub filter: Regex,

    #[structopt(flatten)]
    pub urls: Urls,
}

#[derive(Debug, StructOpt)]
pub struct Urls {
    #[structopt(short, long)]
    pub list: Vec<PathBuf>,

    pub url: Vec<Url>,
}

impl Urls {
    pub async fn collect(&self) -> Result<Vec<Url>, Error> {
        let mut urls = self.url.iter().cloned().collect::<Vec<_>>();

        for list in &self.list {
            let file = File::open(list).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                let url = line.parse()?;
                urls.push(url);
            }
        }

        Ok(urls)
    }
}
