use std::path::PathBuf;

use reqwest::Client;
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
    download::{
        download,
        DownloadArgs,
    },
    scan::{
        scan,
        ScanArgs,
    },
    utils::dedup_urls,
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
        let globals = self.global_args.build()?;

        match self.command {
            Command::Download(args) => download(globals, args).await?,
            Command::Scan(args) => scan(globals, args).await?,
        }

        Ok(())
    }
}

#[derive(Debug, StructOpt)]
pub struct GlobalArgs {
    #[structopt(long, default_value = DEFAULT_USER_AGENT)]
    pub user_agent: String,
}

impl GlobalArgs {
    fn build(self) -> Result<Globals, Error> {
        let client = Client::builder().user_agent(self.user_agent).build()?;
        Ok(Globals { client })
    }
}

pub struct Globals {
    pub client: Client,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    #[structopt(alias = "d")]
    Download(DownloadArgs),
    Scan(ScanArgs),
}

#[derive(Debug, StructOpt)]
pub struct Urls {
    #[structopt(short, long)]
    pub list: Vec<PathBuf>,

    pub url: Vec<Url>,
}

impl Urls {
    pub async fn collect(self) -> Result<Vec<Url>, Error> {
        let mut urls = self.url;

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

        let urls = dedup_urls(urls).collect();
        Ok(urls)
    }
}
