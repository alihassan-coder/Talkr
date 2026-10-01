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
use crate::catalog::{is_valid_model_id, validate_model_id, ModelManifest};
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
    /// Human-readable model name, used in error messages.
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub archive: Option<String>,
    /// Catalog size, used for the disk space check when HEAD gives no length.
    pub size_bytes: u64,
    pub kind: String,
    pub paths: AppPaths,
    pub tx: Sender<DownloadProgress>,
    pub cancel: Arc<AtomicBool>,
}

/// What HEAD told us about the remote file.
#[derive(Default)]
struct RemoteInfo {
    size: u64,
    /// Strong ETag or Last-Modified, sent as `If-Range` so a changed file restarts from zero.
    validator: Option<String>,
}

/// Headroom on top of the computed need, so we never fill the disk to the last byte.
pub(crate) const DISK_MARGIN: u64 = 200 * 1024 * 1024;

/// An archive may expand to at most this many times its own size...
const MAX_EXPANSION_RATIO: u64 = 10;
/// ...and never beyond this, whatever its size. Stops decompression bombs.
const MAX_EXTRACTED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Upper bound on archive entries, so an archive cannot exhaust inodes with empty files.
const MAX_ARCHIVE_ENTRIES: u64 = 100_000;
/// Largest download accepted when neither the catalog nor the server says how big the file is.
/// Same ceiling as extraction: nothing bigger could be installed anyway.
const MAX_DOWNLOAD_BYTES: u64 = MAX_EXTRACTED_BYTES;
/// Slack on top of the catalog size, in case the file is re-uploaded slightly bigger: the
/// larger of 1% and 1 MB. The checksum still has the final say.
const SIZE_SLACK_MIN: u64 = 1024 * 1024;

/// Whether a catalog entry without a valid checksum may still be installed. Debug builds only,
/// so catalog work-in-progress can be tried out; release builds refuse unverifiable files.
const ALLOW_UNVERIFIED: bool = cfg!(debug_assertions);

impl DownloadJob {
    /// Run the download to completion, always emitting a terminal progress event
    /// (`installed`, `failed` or `cancelled`).
    pub async fn run(self) -> Result<()> {
        let result = match self.run_inner().await {
            Err(AppError::Io(e)) if is_disk_full(&e) => Err(AppError::DiskFull(format!(
                "Disk is full, could not finish installing {}. Free up space or delete other models in Models, then try again.",
                self.name
            ))),
            other => other,
        };
        match &result {
            Ok(total) => self.emit(DownloadState::Installed, *total, *total, 0, None, None),
            Err(AppError::Cancelled) => {
                self.remove_partial().await;
                tokio::fs::remove_dir_all(self.scratch_path()).await.ok();
                self.emit(DownloadState::Cancelled, 0, 0, 0, None, None);
            }
            Err(e) => {
                if matches!(e, AppError::DiskFull(_)) {
                    // A resume would hit the same wall; give the space back instead.
                    self.remove_partial().await;
                    tokio::fs::remove_dir_all(self.scratch_path()).await.ok();
                }
                log::error!("Download of {} failed: {}", self.model_id, e);
                self.emit(DownloadState::Failed, 0, 0, 0, None, Some(e.to_string()));
            }
        }
        result.map(|_| ())
    }

    fn part_path(&self) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.part", self.model_id))
    }

    fn scratch_path(&self) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.extract", self.model_id))
    }

    /// Holds the ETag / Last-Modified of the response the `.part` was started from, so a
    /// resume can tell whether the file changed upstream in the meantime.
    fn validator_path(&self) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.part.validator", self.model_id))
    }

    /// Forget the partial download: the `.part` and its validator.
    async fn remove_partial(&self) {
        tokio::fs::remove_file(self.part_path()).await.ok();
        tokio::fs::remove_file(self.validator_path()).await.ok();
    }

    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Resolves once the cancel flag is set. Polled, so a stalled stream can still be cancelled.
    async fn wait_cancel(&self) {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tick.tick().await;
            if self.cancel.load(Ordering::Relaxed) {
                return;
            }
        }
    }

    async fn run_inner(&self) -> Result<u64> {
        // The id becomes file names below; never let it escape the downloads/models dirs.
        validate_model_id(&self.model_id)?;
        let part_path = self.part_path();
        let model_dir = self.paths.model_dir(&self.kind, &self.model_id);

        // Refuse unverifiable files before spending any bandwidth on them.
        let expected = expected_checksum(&self.sha256, ALLOW_UNVERIFIED)?;
        if expected.is_none() {
            log::warn!("{} has no valid sha256 in the catalog; installing unverified (debug build)", self.model_id);
        }

        self.emit(DownloadState::Downloading, 0, 0, 0, None, None);

        let client = http_client(&self.url)?;

        let remote = tokio::select! {
            r = self.get_remote_info(&client) => r.unwrap_or_default(),
            _ = self.wait_cancel() => return Err(AppError::Cancelled),
        };
        let file_size = remote.size;

        let mut received = tokio::fs::metadata(&part_path).await.map(|m| m.len()).unwrap_or(0);
        let saved_validator = tokio::fs::read_to_string(self.validator_path())
            .await
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if received > 0 {
            let too_long = received > max_download_bytes(self.size_bytes, file_size);
            let changed = matches!((&saved_validator, &remote.validator), (Some(saved), Some(now)) if saved != now);
            if too_long || changed {
                // Stale/corrupt partial file, or the file changed upstream: start over.
                log::info!("Discarding partial download of {} (changed upstream or stale)", self.model_id);
                self.remove_partial().await;
                received = 0;
            }
        }

        self.check_disk_space(if file_size > 0 { file_size } else { self.size_bytes }, received)?;

        let mut total = file_size;

        if file_size == 0 || received < file_size {
            let mut request = client.get(&self.url);
            if received > 0 {
                request = request.header(reqwest::header::RANGE, format!("bytes={}-", received));
                // Prefer the validator the .part was started from: with it, If-Range makes the
                // server send the whole (new) file if it changed since.
                if let Some(validator) = saved_validator.as_ref().or(remote.validator.as_ref()) {
                    request = request.header(reqwest::header::IF_RANGE, validator);
                }
            }
            let mut response = tokio::select! {
                r = request.send() => r?,
                _ = self.wait_cancel() => return Err(AppError::Cancelled),
            };
            let status = response.status();

            if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && received > 0 {
                // The .part most likely already holds the whole file (HEAD gave no size);
                // let the checksum decide.
                total = received;
            } else {
                if !status.is_success() {
                    return Err(AppError::Download(format!("HTTP {}", status)));
                }

                let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
                if resumed && content_range_start(response.headers()) != Some(received) {
                    self.remove_partial().await;
                    return Err(AppError::Download(
                        "The server resumed the download at the wrong position; try again".into(),
                    ));
                }
                let mut options = tokio::fs::OpenOptions::new();
                options.create(true);
                if resumed {
                    options.append(true);
                } else {
                    // Server ignored the Range header, the file changed (If-Range), or fresh
                    // download: rewrite from scratch, remembering which version this is.
                    options.write(true).truncate(true);
                    received = 0;
                    match validator_from(response.headers()).or_else(|| remote.validator.clone()) {
                        Some(v) => tokio::fs::write(self.validator_path(), v).await?,
                        None => {
                            tokio::fs::remove_file(self.validator_path()).await.ok();
                        }
                    }
                }
                let mut file = options.open(&part_path).await?;

                total = total.max(received + response.content_length().unwrap_or(0));

                // The checksum only runs once the stream ends, so without a cap a broken or
                // hostile server could fill the disk first. This response's own length is the
                // freshest word on the file's size; HEAD's is the fallback.
                let server_size = match response.content_length() {
                    Some(len) => received.saturating_add(len),
                    None => file_size,
                };
                let limit = max_download_bytes(self.size_bytes, server_size);

                let mut last_emit = Instant::now();
                let mut last_bytes = received;

                loop {
                    let chunk = tokio::select! {
                        c = response.chunk() => c?,
                        _ = self.wait_cancel() => return Err(AppError::Cancelled),
                    };
                    let Some(chunk) = chunk else { break };
                    if received.saturating_add(chunk.len() as u64) > limit {
                        // Close the file first: Windows cannot delete an open one.
                        drop(file);
                        self.remove_partial().await;
                        return Err(AppError::Download(format!(
                            "The server sent more data than expected for {} (over {}); the download was stopped and discarded",
                            self.name,
                            format_bytes(limit)
                        )));
                    }
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
        }

        self.check_cancel()?;

        let path = part_path.clone();
        let hash = match expected {
            Some(expected) => {
                self.emit(DownloadState::Verifying, total, total, 0, None, None);
                let hash = tokio::task::spawn_blocking(move || sha256_file(&path)).await??;
                if hash != expected {
                    self.remove_partial().await;
                    return Err(AppError::Download(format!(
                        "SHA256 mismatch (expected {}, got {})",
                        expected, hash
                    )));
                }
                hash
            }
            // Only reachable in debug builds (refused up front otherwise).
            None => tokio::task::spawn_blocking(move || sha256_file(&path)).await??,
        };

        self.check_cancel()?;

        if model_dir.exists() {
            tokio::fs::remove_dir_all(&model_dir).await?;
        }

        let installed = self.install(&part_path, &model_dir, hash, total).await;
        if installed.is_err() {
            // A dir without a manifest is junk, and a model deleted mid-install must not come back.
            tokio::fs::remove_dir_all(&model_dir).await.ok();
        }
        // The verified .part has been consumed (or is useless): its validator goes with it.
        tokio::fs::remove_file(self.validator_path()).await.ok();
        installed
    }

    /// Move the verified `.part` into `model_dir` (extracting if needed) and write the manifest.
    async fn install(&self, part_path: &Path, model_dir: &Path, hash: String, total: u64) -> Result<u64> {
        if let Some(archive) = self.archive.clone() {
            self.emit(DownloadState::Extracting, total, total, 0, None, None);
            let part = part_path.to_path_buf();
            let dest = model_dir.to_path_buf();
            let scratch = self.scratch_path();
            tokio::task::spawn_blocking(move || extract_archive(&part, &dest, &scratch, &archive)).await??;
            tokio::fs::remove_file(part_path).await.ok();
        } else {
            tokio::fs::create_dir_all(model_dir).await?;
            let file_name = url_file_name(&self.url).unwrap_or("model.bin").to_string();
            tokio::fs::rename(part_path, model_dir.join(file_name)).await?;
        }

        // Extraction can take a while; honour a cancel/delete that arrived meanwhile.
        self.check_cancel()?;

        let dir = model_dir.to_path_buf();
        let model_id = self.model_id.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let (files, size_bytes) = list_dir_files(&dir)?;
            let manifest = ModelManifest {
                id: model_id,
                version: "1.0".into(),
                sha256: hash,
                installed_at: chrono::Utc::now().timestamp_millis(),
                size_bytes,
                files,
            };
            write_manifest(&dir, &manifest)
        })
        .await??;

        Ok(total)
    }

    /// Fail early if the disk holding the app home can't fit the rest of the download plus,
    /// for archives, the extracted files (which coexist with the `.part` for a while).
    /// Skipped when the disk can't be determined.
    fn check_disk_space(&self, file_size: u64, received: u64) -> Result<()> {
        let needed = download_space_needed(file_size, received, self.archive.is_some());
        ensure_free_space(&self.paths.home, needed, &self.name)
    }

    async fn get_remote_info(&self, client: &Client) -> Result<RemoteInfo> {
        let response = client.head(&self.url).timeout(Duration::from_secs(20)).send().await?;
        if !response.status().is_success() {
            return Ok(RemoteInfo::default());
        }
        Ok(RemoteInfo {
            // From the header itself: for a HEAD response the body is empty, so reqwest's
            // `content_length()` (the body's size hint) is not the file size.
            size: response
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0),
            validator: validator_from(response.headers()),
        })
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

/// The most bytes a download may receive. `catalog_size` (plus a little slack) and
/// `server_size` are each 0 when unknown; the smaller known bound wins, and with neither known
/// the hard ceiling applies.
fn max_download_bytes(catalog_size: u64, server_size: u64) -> u64 {
    let from_catalog = (catalog_size > 0)
        .then(|| catalog_size.saturating_add((catalog_size / 100).max(SIZE_SLACK_MIN)));
    let from_server = (server_size > 0).then_some(server_size);
    match (from_catalog, from_server) {
        (Some(c), Some(s)) => c.min(s),
        (Some(c), None) => c,
        (None, Some(s)) => s,
        (None, None) => MAX_DOWNLOAD_BYTES,
    }
}

/// Bytes that must be free to finish a download of `file_size` bytes with `received`
/// already on disk. For archives the extracted files coexist with the `.part` for a while.
fn download_space_needed(file_size: u64, received: u64, is_archive: bool) -> u64 {
    let remaining = file_size.saturating_sub(received);
    // bzip2'd ONNX models expand to roughly 2.5x.
    let expanded = if is_archive { (file_size / 2).saturating_mul(5) } else { 0 };
    remaining.saturating_add(expanded).saturating_add(DISK_MARGIN)
}

/// Fail with a readable [`AppError::DiskFull`] when `available` bytes on `mount` are fewer
/// than `needed`. Pure, so it is testable without a real disk.
fn check_space(what: &str, needed: u64, mount: &Path, available: u64) -> Result<()> {
    if available < needed {
        return Err(AppError::DiskFull(format!(
            "Not enough disk space to install {}: needs about {} free on {}, only {} available. Free up space or delete other models in Models.",
            what,
            format_bytes(needed),
            mount.display(),
            format_bytes(available)
        )));
    }
    Ok(())
}

/// Fail early if the disk holding `at` (an existing path) has fewer than `needed` bytes free.
/// Skipped when the disk can't be determined. `what` names the model in the error message.
pub(crate) fn ensure_free_space(at: &Path, needed: u64, what: &str) -> Result<()> {
    match free_space_for(at) {
        Some((mount, available)) => check_space(what, needed, &mount, available),
        None => Ok(()),
    }
}

/// Mount point and free bytes of the disk holding `path` (longest mount-point prefix wins).
fn free_space_for(path: &Path) -> Option<(PathBuf, u64)> {
    let canonical = std::fs::canonicalize(path).ok()?;
    // Windows canonicalizes to `\\?\C:\...`, while sysinfo reports mount points as `C:\`.
    let canonical = match canonical.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(stripped) if !stripped.starts_with("UNC") => PathBuf::from(stripped),
        _ => canonical,
    };
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| canonical.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().components().count())
        .map(|d| (d.mount_point().to_path_buf(), d.available_space()))
}

fn format_bytes(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", b / MB)
    }
}

fn is_disk_full(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::StorageFull {
        return true;
    }
    match e.raw_os_error() {
        #[cfg(unix)]
        Some(28) => true, // ENOSPC
        #[cfg(windows)]
        Some(112) | Some(39) => true, // ERROR_DISK_FULL, ERROR_HANDLE_DISK_FULL
        _ => false,
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// The normalized (lowercase) expected checksum. A missing or malformed checksum is an
/// error unless `allow_unverified`, in which case it yields `None`.
fn expected_checksum(sha256: &str, allow_unverified: bool) -> Result<Option<String>> {
    let expected = sha256.trim().to_ascii_lowercase();
    if is_sha256(&expected) {
        Ok(Some(expected))
    } else if allow_unverified {
        Ok(None)
    } else {
        Err(AppError::Download(
            "This model has no checksum in the catalog, refusing to install an unverified file".into(),
        ))
    }
}

/// The HTTP client for model downloads: https only, with connect and read timeouts.
fn http_client(url: &str) -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!("Talkr/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(30))
        .https_only(!plain_http_allowed(url))
        .build()?)
}

/// Plain http is allowed only in unit-test builds, and only to a loopback host (the
/// in-process test server). Always `false` in the app.
fn plain_http_allowed(url: &str) -> bool {
    cfg!(test) && is_loopback_http(url)
}

fn is_loopback_http(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else { return false };
    if url.scheme() != "http" {
        return false;
    }
    match url.host_str() {
        Some(host) if host.eq_ignore_ascii_case("localhost") => true,
        Some(host) => host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback()),
        None => false,
    }
}

/// Strong ETag, or else Last-Modified: what `If-Range` accepts (it rejects weak ETags).
fn validator_from(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let header = |name: reqwest::header::HeaderName| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    header(reqwest::header::ETAG)
        .filter(|e| !e.starts_with("W/"))
        .or_else(|| header(reqwest::header::LAST_MODIFIED))
}

/// First byte position of a `Content-Range: bytes N-M/T` header.
fn content_range_start(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let range = value.trim().strip_prefix("bytes")?.trim_start();
    range.split('-').next()?.trim().parse().ok()
}

/// The last path segment of `url`, if it is a safe file name.
fn url_file_name(url: &str) -> Option<&str> {
    let path = url.split(['?', '#']).next()?;
    path.rsplit('/').next().filter(|name| is_valid_model_id(name))
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
    let result = extract_into(archive_path, dest, scratch, format);
    // Clean up on success and failure alike. `dest` only ever appears through the final
    // rename, so a failed extraction never leaves a half-populated model dir.
    if scratch.exists() {
        std::fs::remove_dir_all(scratch).ok();
    }
    result
}

fn extract_into(archive_path: &Path, dest: &Path, scratch: &Path, format: &str) -> Result<()> {
    if scratch.exists() {
        std::fs::remove_dir_all(scratch)?;
    }
    std::fs::create_dir_all(scratch)?;

    let limit = extraction_limit(std::fs::metadata(archive_path)?.len());
    let file = std::io::BufReader::new(std::fs::File::open(archive_path)?);
    match format {
        "tar.bz2" | "tbz2" => unpack_checked(tar::Archive::new(bzip2::read::BzDecoder::new(file)), scratch, limit)?,
        "tar" => unpack_checked(tar::Archive::new(file), scratch, limit)?,
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
    Ok(())
}

/// How many bytes an archive of `archive_len` bytes may expand to.
fn extraction_limit(archive_len: u64) -> u64 {
    archive_len.saturating_mul(MAX_EXPANSION_RATIO).min(MAX_EXTRACTED_BYTES)
}

fn unsafe_archive(why: String) -> AppError {
    AppError::Download(format!("Refusing to extract the downloaded archive: {}", why))
}

/// An entry path must stay inside the destination: relative, no `..`, no drive or root,
/// and (for NTFS) no `:` that would address an alternate data stream.
fn check_entry_path(path: &Path) -> Result<()> {
    use std::path::Component;
    let mut normal = 0;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy().contains(':') {
                    return Err(unsafe_archive(format!("entry {} has an invalid name", path.display())));
                }
                normal += 1;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(unsafe_archive(format!("entry {} points outside the model folder", path.display())));
            }
        }
    }
    if normal == 0 {
        return Err(unsafe_archive("an entry has an empty path".into()));
    }
    Ok(())
}

/// Unpack only plain files and directories with safe paths into `dest`, stopping as soon
/// as the declared sizes add up to more than `limit` bytes (before those bytes are written).
fn unpack_checked<R: std::io::Read>(mut archive: tar::Archive<R>, dest: &Path, limit: u64) -> Result<()> {
    use tar::EntryType;
    let mut total = 0u64;
    let mut count = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        match entry.header().entry_type() {
            // pax global headers carry metadata only (tar's own unpack skips them too).
            EntryType::XGlobalHeader => continue,
            EntryType::Regular | EntryType::Continuous | EntryType::Directory => {}
            other => {
                return Err(unsafe_archive(format!(
                    "entry {} is a {:?}, only plain files and folders are allowed",
                    path.display(),
                    other
                )))
            }
        }
        check_entry_path(&path)?;

        count += 1;
        if count > MAX_ARCHIVE_ENTRIES {
            return Err(unsafe_archive(format!("more than {} entries", MAX_ARCHIVE_ENTRIES)));
        }
        total = total.saturating_add(entry.size());
        if total > limit {
            return Err(unsafe_archive(format!(
                "it expands to more than {} (possible decompression bomb)",
                format_bytes(limit)
            )));
        }

        if !entry.unpack_in(dest)? {
            return Err(unsafe_archive(format!("entry {} points outside the model folder", path.display())));
        }
    }
    Ok(())
}

/// Write `manifest.json` into `dir`, the step that marks a model as installed. The model files
/// are flushed to disk first and the manifest is written atomically, so after a crash or power
/// loss a manifest that parses always describes files that are really there; a torn one simply
/// reads as "not installed".
pub(crate) fn write_manifest(dir: &Path, manifest: &ModelManifest) -> Result<()> {
    sync_tree(dir)?;
    let json = serde_json::to_string_pretty(manifest)?;
    crate::config::write_atomic(&dir.join("manifest.json"), json.as_bytes())?;
    sync_dir(dir);
    Ok(())
}

/// Flush every file under `dir` (and, where the OS allows, the folders) to disk.
fn sync_tree(dir: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            // Flushing needs write access on Windows (FlushFileBuffers). Opening for write
            // does not truncate or change the file.
            let file = match std::fs::OpenOptions::new().write(true).open(entry.path()) {
                Ok(file) => file,
                // A read-only file from an archive: nothing we can flush, and not worth
                // failing the install over.
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => continue,
                Err(e) => return Err(e.into()),
            };
            file.sync_all()?;
        } else if entry.file_type().is_dir() {
            sync_dir(entry.path());
        }
    }
    Ok(())
}

/// Make new or renamed entries in `dir` durable. Only possible (and only needed) on Unix;
/// best effort, since some file systems refuse it.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
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

#[cfg(test)]
#[path = "downloader_tests.rs"]
pub(crate) mod tests;
