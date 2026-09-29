use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use flume::{Sender, Receiver};
use reqwest::Client;
use sha2::{Sha256, Digest};
use tokio::fs;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use crate::error::{AppError, Result};
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub model_id: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub eta_sec: Option<u64>,
    pub state: DownloadState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadState {
    Queued,
    Downloading,
    Verifying,
    Extracting,
    Installed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub model_id: String,
    pub url: String,
    pub sha256: String,
    pub archive: Option<String>,
    pub kind: String,
    pub paths: AppPaths,
    pub tx: Sender<DownloadProgress>,
    pub cancel_rx: Receiver<()>,
}

impl DownloadJob {
    pub async fn run(self) -> Result<()> {
        let mut hasher = Sha256::new();
        let part_path = self.paths.cache_downloads.join(format!("{}.part", self.model_id));
        let model_dir = self.paths.model_dir(&self.kind, &self.model_id);

        self.emit(DownloadProgress {
            model_id: self.model_id.clone(),
            received_bytes: 0,
            total_bytes: 0,
            bytes_per_sec: 0,
            eta_sec: None,
            state: DownloadState::Downloading,
        });

        let client = Client::new();
        let mut received: u64 = 0;
        let mut last_progress = Instant::now();
        let mut last_bytes = 0u64;

        let file_size = self.get_file_size(&client).await?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part_path)
            .await?;

        received = file.metadata().await?.len();

        let mut request = client.get(&self.url);
        if received > 0 {
            request = request.header("Range", format!("bytes={}-", received));
        }

        let mut response = request.send().await?;

        if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(AppError::Download(format!("HTTP {}", response.status())));
        }

        let total = file_size.max(received + response.content_length().unwrap_or(0));

        while let Some(chunk) = response.chunk().await? {
            tokio::select! {
                _ = self.cancel_rx.recv_async() => {
                    self.emit(DownloadProgress {
                        model_id: self.model_id.clone(),
                        received_bytes: received,
                        total_bytes: total,
                        bytes_per_sec: 0,
                        eta_sec: None,
                        state: DownloadState::Cancelled,
                    });
                    return Err(AppError::Cancelled);
                }
                _ = async {
                    file.write_all(&chunk).await?;
                    hasher.update(&chunk);
                    received += chunk.len() as u64;

                    let now = Instant::now();
                    if now.duration_since(last_progress) >= Duration::from_millis(250) {
                        let elapsed = now.duration_since(last_progress).as_secs_f64();
                        let bytes_per_sec = ((received - last_bytes) as f64 / elapsed) as u64;
                        let eta = if bytes_per_sec > 0 {
                            Some(((total - received) / bytes_per_sec) as u64)
                        } else {
                            None
                        };

                        self.emit(DownloadProgress {
                            model_id: self.model_id.clone(),
                            received_bytes: received,
                            total_bytes: total,
                            bytes_per_sec,
                            eta_sec: eta,
                            state: DownloadState::Downloading,
                        });

                        last_progress = now;
                        last_bytes = received;
                    }
                    Ok::<(), AppError>(())
                } => {}
            }
        }

        file.flush().await?;
        drop(file);

        self.emit(DownloadProgress {
            model_id: self.model_id.clone(),
            received_bytes: total,
            total_bytes: total,
            bytes_per_sec: 0,
            eta_sec: None,
            state: DownloadState::Verifying,
        });

        let hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();
        if hash.to_lowercase() != self.sha256.to_lowercase() {
            fs::remove_file(&part_path).await.ok();
            return Err(AppError::Download("SHA256 mismatch".into()));
        }

        if let Some(archive) = self.archive {
            self.emit(DownloadProgress {
                model_id: self.model_id.clone(),
                received_bytes: total,
                total_bytes: total,
                bytes_per_sec: 0,
                eta_sec: None,
                state: DownloadState::Extracting,
            });

            Self::extract_archive(&part_path, &model_dir, &archive).await?;
        } else {
            fs::create_dir_all(&model_dir).await?;
            fs::rename(&part_path, model_dir.join(Path::new(&self.url).file_name().unwrap())).await?;
        }

        let manifest = crate::catalog::ModelManifest {
            id: self.model_id.clone(),
            version: "1.0".into(),
            sha256: hash,
            installed_at: chrono::Utc::now().timestamp_millis(),
            size_bytes: total,
            files: vec![],
        };

        fs::write(model_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?).await?;

        fs::remove_file(&part_path).await.ok();

        self.emit(DownloadProgress {
            model_id: self.model_id.clone(),
            received_bytes: total,
            total_bytes: total,
            bytes_per_sec: 0,
            eta_sec: None,
            state: DownloadState::Installed,
        });

        Ok(())
    }

    async fn get_file_size(&self, client: &Client) -> Result<u64> {
        let response = client.head(&self.url).send().await?;
        Ok(response.content_length().unwrap_or(0))
    }

    async fn extract_archive(part_path: &Path, dest: &Path, format: &str) -> Result<()> {
        fs::create_dir_all(dest).await?;

        let file = fs::File::open(part_path).await?;
        let mut archive = tar::Archive::new(
            bzip2::read::BzDecoder::new(file)
        );
        archive.unpack(dest)?;

        Ok(())
    }

    fn emit(&self, progress: DownloadProgress) {
        let _ = self.tx.send(progress);
    }
}

pub struct DownloadManager {
    pub tx: Sender<(String, DownloadJob)>,
    pub cancel_tx: Sender<(String, ())>,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();
        let (cancel_tx, cancel_rx) = flume::unbounded();

        let manager = Self { tx, cancel_tx };

        tokio::spawn(Self::run_worker(rx, cancel_rx));

        manager
    }

    async fn run_worker(rx: Receiver<(String, DownloadJob)>, cancel_rx: Receiver<(String, ())>) {
        let mut active: Vec<(String, tokio::task::JoinHandle<()>)> = Vec::new();
        let mut queued: Vec<(String, DownloadJob)> = Vec::new();

        loop {
            tokio::select! {
                Some((id, job)) = rx.recv_async() => {
                    if active.len() < 2 {
                        let handle = tokio::spawn(async move {
                            let _ = job.run().await;
                        });
                        active.push((id, handle));
                    } else {
                        queued.push((id, job));
                    }
                }
                Some((id, _)) = cancel_rx.recv_async() => {
                    active.retain(|(job_id, handle)| {
                        if job_id == &id {
                            handle.abort();
                            false
                        } else {
                            true
                        }
                    });
                    queued.retain(|(job_id, _)| job_id != &id);
                }
                _ = async {
                    if !active.is_empty() {
                        let mut completed = Vec::new();
                        for (i, (id, handle)) in active.iter().enumerate() {
                            if handle.is_finished() {
                                completed.push(i);
                            }
                        }
                        for &i in completed.iter().rev() {
                            active.remove(i);
                        }

                        while active.len() < 2 && !queued.is_empty() {
                            let (id, job) = queued.remove(0);
                            let handle = tokio::spawn(async move {
                                let _ = job.run().await;
                            });
                            active.push((id, handle));
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                } => {}
            }
        }
    }

    pub fn queue_download(&self, job: DownloadJob) {
        let _ = self.tx.send((job.model_id.clone(), job));
    }

    pub fn cancel(&self, model_id: &str) {
        let _ = self.cancel_tx.send((model_id.to_string(), ()));
    }
}