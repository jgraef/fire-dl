use std::{
    collections::{
        HashMap,
        HashSet,
    },
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use color_eyre::eyre::bail;
pub use color_eyre::eyre::Error;
use futures::StreamExt;
use indicatif::{
    MultiProgress,
    ProgressBar,
    ProgressStyle,
};
use reqwest::{
    Client,
    Response,
};
use tokio::{
    fs::File,
    io::{
        AsyncWriteExt,
        BufWriter,
    },
};
use url::Url;

use crate::args::{
    DownloadArgs,
    GlobalArgs,
};

pub async fn download(global: GlobalArgs, args: DownloadArgs) -> Result<(), Error> {
    let output = args.output.as_deref().unwrap_or_else(|| Path::new("."));
    if !output.exists() {
        bail!("output path does not exist");
    }
    if !output.metadata()?.is_dir() {
        bail!("output path is not a directory");
    }
    tracing::debug!(output = %output.display());

    let urls = args.urls.collect().await?;

    let mut jobs = vec![];
    let mut id = 0;

    let mut output_files = HashMap::new();
    let mut urls_seen = HashSet::new();

    let client = Client::builder().user_agent(global.user_agent).build()?;

    for url in urls.into_iter() {
        // check if we already have a job for that url
        if urls_seen.contains(&url) {
            continue;
        }
        else {
            urls_seen.insert(url.clone());
        }

        // todo: handle missing file name
        let mut file_name = file_name_from_url(&url).unwrap().to_owned();

        if let Some(suffix) = output_files.get_mut(&file_name) {
            file_name = format!("{file_name}.{suffix}");
            *suffix += 1;
        }
        else {
            output_files.insert(file_name.clone(), 2);
        }

        let mut unlink_existing = false;
        let path = output.join(&file_name);
        if path.exists() {
            if args.redownload_existing {
                tracing::info!(file_name, "file exists. redownloading.");
                unlink_existing = true;
            }
            else {
                tracing::info!(file_name, "file exists. skipping.");
                continue;
            }
        }

        let job = Job {
            client: client.clone(),
            id,
            url,
            file_name,
            path,
            unlink_existing,
        };

        id += 1;
        jobs.push(job);
    }

    println!("downloading {} files", jobs.len());

    let progress = Progress::new(jobs.len());

    futures::stream::iter(&jobs)
        .map(|job| {
            let span = tracing::info_span!("download", id = job.id);
            let _guard = span.enter();
            let progress = progress.clone();
            async { job.clone().run(progress).await }
        })
        .buffer_unordered(args.parallel)
        .collect::<()>()
        .await;

    Ok(())
}

fn file_name_from_url(url: &Url) -> Option<&str> {
    url.path_segments().and_then(|iter| iter.last())
}

#[derive(Clone)]
struct Progress {
    multi_progress: MultiProgress,
    progress_style: Arc<ProgressStyle>,
    spinner_style: Arc<ProgressStyle>,
    num_jobs: usize,
}

impl Progress {
    fn new(num_jobs: usize) -> Self {
        let multi_progress = MultiProgress::new();

        let progress_style = ProgressStyle::with_template(
            "{prefix} {spinner:.green} {msg} [{elapsed_precise}] {wide_bar:.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec}, {eta})"
        )
            .unwrap()
            .progress_chars("#>-");

        let spinner_style = ProgressStyle::with_template(
            "{prefix} {spinner:.green} {msg} [{elapsed_precise}] {bytes} ({bytes_per_sec})",
        )
        .unwrap();

        Self {
            multi_progress,
            progress_style: Arc::new(progress_style),
            spinner_style: Arc::new(spinner_style),
            num_jobs,
        }
    }

    fn add(self, id: usize, file_name: impl Into<String>, length: Option<u64>) -> ProgressBar {
        let progress_bar;
        if let Some(length) = length {
            progress_bar = self.multi_progress.add(ProgressBar::new(length));
            progress_bar.set_style(ProgressStyle::clone(&self.progress_style));
        }
        else {
            progress_bar = ProgressBar::new_spinner();
            progress_bar.set_style(ProgressStyle::clone(&self.spinner_style));
        }
        progress_bar.set_message(file_name.into());
        progress_bar.set_prefix(format!("[{}/{}]", id + 1, self.num_jobs));
        progress_bar
    }
}

#[derive(Clone)]
struct Job {
    client: Client,
    id: usize,
    url: Url,
    file_name: String,
    path: PathBuf,
    unlink_existing: bool,
}

impl Job {
    async fn run(self, progress: Progress) {
        // todo: handle this error
        let response = match self.client.get(self.url.clone()).send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(file_name = self.file_name, "failed: {error}");
                return;
            }
        };

        let progress_bar = progress.add(self.id, &self.file_name, response.content_length());

        if let Err(error) = self.download(response, &progress_bar).await {
            progress_bar.abandon_with_message(format!("failed: {}: {error}", self.file_name));
        }
        else {
            progress_bar.finish_and_clear();
        }
    }

    async fn download(
        &self,
        mut response: Response,
        progress_bar: &ProgressBar,
    ) -> Result<(), Error> {
        let temp_path = self
            .path
            .parent()
            .expect("output path has no parent directory")
            .join(format!(".{}.part", self.file_name));

        let file = File::create(&temp_path).await?;
        let mut writer = BufWriter::new(file);

        while let Some(chunk) = response.chunk().await? {
            writer.write_all(&chunk).await?;
            progress_bar.inc(chunk.len() as _);
        }

        writer.flush().await?;

        if self.unlink_existing {
            tokio::fs::remove_file(&self.path).await?;
        }
        tokio::fs::rename(temp_path, &self.path).await?;

        Ok(())
    }
}
