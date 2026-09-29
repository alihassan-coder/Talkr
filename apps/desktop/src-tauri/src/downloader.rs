use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use flume::Sender;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use crate::catalog::ModelManifest;
use crate::error::{AppError, Result};
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub job_id: String,
    pub model_id: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub eta_sec: Option<u64>,
    pub state: DownloadState,
    pub error: Option<String>,
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

pub struct DownloadJob {
    pub job_id: String,
    pub model_id: String,
    pub url: String,
    pub sha256: String,
    pub archive: Option<String>,
    pub kind: String,
    pub paths: AppPaths,
    pub tx: Sender<DownloadProgress>,
    pub cancel: Arc<AtomicBool>,
}

impl DownloadJob {
    /// Run the download to completion, always emitting a terminal progress event
    /// (`installed`, `failed` or `cancelled`).
    pub async fn run(self) -> Result<()> {
        let result = self.run_inner().await;
        let part_path = self.part_path();
        match &result {
            Ok(total) => self.emit(DownloadState::Installed, *total, *total, 0, None, None),
            Err(AppError::Cancelled) => {
                tokio::fs::remove_file(&part_path).await.ok();
                self.emit(DownloadState::Cancelled, 0, 0, 0, None, None);
            }
            Err(e) => {
                log::error!("Download of {} failed: {}", self.model_id, e);
                self.emit(DownloadState::Failed, 0, 0, 0, None, Some(e.to_string()));
            }
        }
        result.map(|_| ())
    }

    fn part_path(&self) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.part", self.model_id))
    }

    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    async fn run_inner(&self) -> Result<u64> {
        let part_path = self.part_path();
        let model_dir = self.paths.model_dir(&self.kind, &self.model_id);

        self.emit(DownloadState::Downloading, 0, 0, 0, None, None);

        let client = Client::builder()
            .user_agent(concat!("Talkr/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(30))
            .build()?;

        let file_size = self.get_file_size(&client).await.unwrap_or(0);

        let mut received = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
        if file_size > 0 && received > file_size {
            // Stale/corrupt partial file: start over.
            tokio::fs::remove_file(&part_path).await.ok();
            received = 0;
        }

        let mut total = file_size;

        if file_size == 0 || received < file_size {
            let mut request = client.get(&self.url);
            if received > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={}-", received));
            }
            let mut response = request.send().await?;
            let status = response.status();
            if !status.is_success() {
                return Err(AppError::Download(format!("HTTP {}", status)));
            }

            let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
            let mut options = tokio::fs::OpenOptions::new();
            options.create(true);
            if resumed {
                options.append(true);
            } else {
                // Server ignored the Range header (or fresh download): rewrite from scratch.
                options.write(true).truncate(true);
                received = 0;
            }
            let mut file = options.open(&part_path).await?;

            total = total.max(received + response.content_length().unwrap_or(0));

            let mut last_emit = Instant::now();
            let mut last_bytes = received;

            while let Some(chunk) = response.chunk().await? {
                self.check_cancel()?;
                file.write_all(&chunk).await?;
                received += chunk.len() as u64;

                let now = Instant::now();
                let elapsed = now.duration_since(last_emit);
                if elapsed >= Duration::from_millis(250) {
                    let bytes_per_sec = ((received - last_bytes) as f64 / elapsed.as_secs_f64()) as u64;
                    let eta = (bytes_per_sec > 0 && total > 0)
                        .then(|| total.saturating_sub(received) / bytes_per_sec);
                    self.emit(DownloadState::Downloading, received, total, bytes_per_sec, eta, None);
                    last_emit = now;
                    last_bytes = received;
                }
            }

            file.flush().await?;
            drop(file);
            total = total.max(received);
        }

        self.check_cancel()?;

        // Verify checksum (skipped for catalog entries without a real sha256 yet).
        let expected = self.sha256.trim().to_lowercase();
        let hash = if is_sha256(&expected) {
            self.emit(DownloadState::Verifying, total, total, 0, None, None);
            let path = part_path.clone();
            let hash = tokio::task::spawn_blocking(move || sha256_file(&path)).await??;
            if hash != expected {
                tokio::fs::remove_file(&part_path).await.ok();
                return Err(AppError::Download(format!(
                    "SHA256 mismatch (expected {}, got {})",
                    expected, hash
                )));
            }
            hash
        } else {
            let path = part_path.clone();
            tokio::task::spawn_blocking(move || sha256_file(&path)).await??
        };

        self.check_cancel()?;

        if model_dir.exists() {
            tokio::fs::remove_dir_all(&model_dir).await?;
        }

        if let Some(archive) = self.archive.clone() {
            self.emit(DownloadState::Extracting, total, total, 0, None, None);
            let part = part_path.clone();
            let dest = model_dir.clone();
            let scratch = self.paths.cache_downloads.join(format!("{}.extract", self.model_id));
            tokio::task::spawn_blocking(move || extract_archive(&part, &dest, &scratch, &archive)).await??;
            tokio::fs::remove_file(&part_path).await.ok();
        } else {
            tokio::fs::create_dir_all(&model_dir).await?;
            let file_name = self
                .url
                .rsplit('/')
                .next()
                .and_then(|s| s.split('?').next())
                .filter(|s| !s.is_empty())
                .unwrap_or("model.bin")
                .to_string();
            tokio::fs::rename(&part_path, model_dir.join(file_name)).await?;
        }

        let dir = model_dir.clone();
        let (files, size_bytes) = tokio::task::spawn_blocking(move || list_dir_files(&dir)).await??;

        let manifest = ModelManifest {
            id: self.model_id.clone(),
            version: "1.0".into(),
            sha256: hash,
            installed_at: chrono::Utc::now().timestamp_millis(),
            size_bytes,
            files,
        };

        tokio::fs::write(model_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?).await?;

        Ok(total)
    }

    async fn get_file_size(&self, client: &Client) -> Result<u64> {
        let response = client.head(&self.url).send().await?;
        if !response.status().is_success() {
            return Ok(0);
        }
        Ok(response.content_length().unwrap_or(0))
    }

    fn emit(
        &self,
        state: DownloadState,
        received_bytes: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
        eta_sec: Option<u64>,
        error: Option<String>,
    ) {
        let _ = self.tx.send(DownloadProgress {
            job_id: self.job_id.clone(),
            model_id: self.model_id.clone(),
            received_bytes,
            total_bytes,
            bytes_per_sec,
            eta_sec,
            state,
            error,
        });
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

/// Extract `archive_path` into `dest`. Archives with a single top-level directory
/// (as sherpa-onnx releases are) are flattened so the model files land directly in `dest`.
fn extract_archive(archive_path: &Path, dest: &Path, scratch: &Path, format: &str) -> Result<()> {
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    std::fs::create_dir_all(scratch)?;

    let file = std::io::BufReader::new(std::fs::File::open(archive_path)?);
    match format {
        "tar.bz2" | "tbz2" => tar::Archive::new(bzip2::read::BzDecoder::new(file)).unpack(scratch)?,
        "tar" => tar::Archive::new(file).unpack(scratch)?,
        other => return Err(AppError::Download(format!("Unsupported archive format: {}", other))),
    }

    let entries: Vec<PathBuf> = std::fs::read_dir(scratch)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    let root = if entries.len() == 1 && entries[0].is_dir() {
        entries[0].clone()
    } else {
        scratch.to_path_buf()
    };

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&root, dest)?;
    if scratch.exists() {
        std::fs::remove_dir_all(scratch).ok();
    }
    Ok(())
}

/// Relative file paths (forward slashes) and total size of a directory tree.
pub fn list_dir_files(dir: &Path) -> Result<(Vec<String>, u64)> {
    let mut files = Vec::new();
    let mut size = 0u64;
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            size += entry.metadata()?.len();
            if let Ok(rel) = entry.path().strip_prefix(dir) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort();
    Ok((files, size))
}

struct ActiveDownload {
    model_id: String,
    cancel: Arc<AtomicBool>,
}

/// Tracks in-flight downloads so they can be cancelled and de-duplicated.
#[derive(Default)]
pub struct DownloadManager {
    jobs: Mutex<HashMap<String, ActiveDownload>>,
}

impl DownloadManager {
    /// Register a new job. Fails if `model_id` is already downloading.
    pub fn register(&self, job_id: &str, model_id: &str) -> Result<Arc<AtomicBool>> {
        let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        if jobs.values().any(|j| j.model_id == model_id) {
            return Err(AppError::Download(format!("{} is already downloading", model_id)));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.insert(
            job_id.to_string(),
            ActiveDownload {
                model_id: model_id.to_string(),
                cancel: cancel.clone(),
            },
        );
        Ok(cancel)
    }

    pub fn finish(&self, job_id: &str) {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).remove(job_id);
    }

    /// Cancel by job id or model id. Returns whether a matching job was found.
    pub fn cancel(&self, id: &str) -> bool {
        let jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
        let mut found = false;
        for (job_id, job) in jobs.iter() {
            if job_id == id || job.model_id == id {
                job.cancel.store(true, Ordering::Relaxed);
                found = true;
            }
        }
        found
    }

    pub fn is_downloading(&self, model_id: &str) -> bool {
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|j| j.model_id == model_id)
    }
}
