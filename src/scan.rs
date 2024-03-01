use std::{
    path::PathBuf,
    sync::Arc,
};

use futures::StreamExt;
use regex::Regex;
use reqwest::{
    header,
    Client,
};
use soup::{
    NodeExt,
    QueryBuilderExt,
    Soup,
};
use structopt::StructOpt;
use tokio::sync::mpsc::{
    unbounded_channel,
    UnboundedSender,
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use url::Url;

use crate::{
    args::{
        Globals,
        Urls,
    },
    Error,
};

#[derive(Debug, StructOpt)]
pub struct ScanArgs {
    #[structopt(short, long)]
    pub output: Option<PathBuf>,

    #[structopt(flatten)]
    pub filters: Filters,

    #[structopt(short, long, default_value = "1")]
    pub parallel: usize,

    #[structopt(flatten)]
    pub urls: Urls,
}

pub async fn scan(globals: Globals, args: ScanArgs) -> Result<(), Error> {
    let (job_tx, job_rx) = unbounded_channel();
    let (output_tx, mut output_rx) = unbounded_channel();

    let scan = Scan {
        filters: Arc::new(args.filters),
        client: globals.client.clone(),
        output_tx,
    };

    for url in args.urls.collect().await? {
        job_tx
            .send(Job {
                url,
                scan: scan.clone(),
            })
            .unwrap();
    }

    // we need to drop the job_tx and output_tx, otherwise the input stream will
    // never end
    drop(job_tx);
    drop(scan);

    UnboundedReceiverStream::new(job_rx)
        .map(|job| {
            async move {
                let span = tracing::info_span!("scan", url = %job.url);
                let _guard = span.enter();
                if let Err(error) = job.run().await {
                    tracing::error!("{error}");
                }
            }
        })
        .buffer_unordered(args.parallel)
        .collect::<()>()
        .await;

    while let Some(url) = output_rx.recv().await {
        println!("{url}");
    }

    Ok(())
}

#[derive(Debug, StructOpt)]
pub struct Filters {
    #[structopt(long)]
    filter_url: Vec<Regex>,
}

impl Filters {
    pub fn is_match(&self, url: &Url) -> bool {
        let url_str = url.to_string();
        self.filter_url.iter().any(|regex| regex.is_match(&url_str))
    }
}

#[derive(Clone)]
struct Scan {
    filters: Arc<Filters>,
    client: Client,
    output_tx: UnboundedSender<Url>,
}

struct Job {
    url: Url,
    scan: Scan,
}

impl Job {
    async fn run(self) -> Result<(), Error> {
        let response = self.scan.client.get(self.url.clone()).send().await?;

        let headers = response.headers();
        let content_type = headers.get(header::CONTENT_TYPE);

        let urls = if let Some(content_type) = content_type {
            match content_type.to_str()? {
                "text/html" => {
                    let html = response.text().await?;
                    scan_html(&html, &self.url)
                }
                _ => vec![],
            }
        }
        else {
            vec![]
        };

        for url in urls {
            if self.scan.filters.is_match(&url) {
                self.scan.output_tx.send(url).unwrap();
            }
        }

        Ok(())
    }
}

fn scan_html(html: &str, base_url: &Url) -> Vec<Url> {
    let soup = Soup::new(html);
    let mut urls = vec![];

    for a in soup.tag("a").find_all() {
        if let Some(href) = a.get("href") {
            if let Ok(url) = base_url.join(&href) {
                urls.push(url);
            }
        }
    }

    urls
}
