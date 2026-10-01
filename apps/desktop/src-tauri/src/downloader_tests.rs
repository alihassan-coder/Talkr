//! Tests for `downloader.rs`: pure helpers, archive extraction safety, and end-to-end
//! downloads against a tiny in-process HTTP server on the loopback interface.

use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use tar::{EntryType, Header};

// ---------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------

/// An `AppPaths` rooted in `root`, with every directory created (like `AppPaths::init`).
pub(crate) fn test_paths(root: &Path) -> AppPaths {
    let home = root.join(".talkr");
    let paths = AppPaths {
        home: home.clone(),
        models: home.join("models"),
        models_stt: home.join("models").join("stt"),
        models_tts: home.join("models").join("tts"),
        audio: home.join("audio"),
        history: home.join("history"),
        cache: home.join("cache"),
        cache_downloads: home.join("cache").join("downloads"),
        logs: home.join("logs"),
        config_file: home.join("config.json"),
        db_file: home.join("history").join("talkr.db"),
    };
    for dir in [
        &paths.models_stt,
        &paths.models_tts,
        &paths.audio,
        &paths.history,
        &paths.cache_downloads,
        &paths.logs,
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    paths
}

fn sha_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Deterministic, poorly compressible test data.
fn pseudo_random(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 24) as u8
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// Tiny HTTP/1.1 server: HEAD, GET, Range: bytes=N-, If-Range, 416, stalls.
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Logged {
    method: String,
    range: Option<String>,
    if_range: Option<String>,
    status: u16,
}

struct ServerState {
    body: Vec<u8>,
    etag: Option<String>,
    /// Status for HEAD; anything but 200 means "HEAD not supported".
    head_status: u16,
    /// Status for GET when not 200/206 (e.g. 404).
    get_status: Option<u16>,
    /// Send this many body bytes, then stop sending (connection stays open for a while).
    stall_after: Option<usize>,
    /// Answer range requests with a 206 that starts at byte 0 (a broken server).
    wrong_range_start: bool,
    /// Send GET bodies without a Content-Length (the body ends when the connection closes).
    no_length: bool,
    /// Size HEAD reports instead of the real one.
    head_len: Option<usize>,
    log: Vec<Logged>,
}

struct TestServer {
    addr: SocketAddr,
    state: Arc<Mutex<ServerState>>,
}

impl TestServer {
    fn start(body: Vec<u8>, etag: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(ServerState {
            body,
            etag: etag.map(str::to_string),
            head_status: 200,
            get_status: None,
            stall_after: None,
            wrong_range_start: false,
            no_length: false,
            head_len: None,
            log: Vec::new(),
        }));
        let shared = state.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let state = shared.clone();
                std::thread::spawn(move || {
                    handle_connection(stream, &state).ok();
                });
            }
        });
        TestServer { addr, state }
    }

    fn url(&self, file: &str) -> String {
        format!("http://{}/models/{}", self.addr, file)
    }

    fn with<T>(&self, f: impl FnOnce(&mut ServerState) -> T) -> T {
        f(&mut self.state.lock().unwrap())
    }

    fn log(&self) -> Vec<Logged> {
        self.with(|s| s.log.clone())
    }

    fn gets(&self) -> Vec<Logged> {
        self.log().into_iter().filter(|r| r.method == "GET").collect()
    }
}

fn handle_connection(mut stream: TcpStream, state: &Mutex<ServerState>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    // One request per connection (we answer with `Connection: close`).
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let mut lines = text.split("\r\n");
    let method = lines.next().unwrap_or("").split(' ').next().unwrap_or("").to_string();
    let header = |name: &str| -> Option<String> {
        text.split("\r\n")
            .skip(1)
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
            .map(|(_, v)| v.trim().to_string())
    };
    let range = header("range");
    let if_range = header("if-range");

    let (status, headers, body, stall_after) = {
        let s = state.lock().unwrap();
        let len = s.body.len();
        let mut headers = vec![("Accept-Ranges".to_string(), "bytes".to_string())];
        if let Some(etag) = &s.etag {
            headers.push(("ETag".into(), etag.clone()));
        }
        let (status, body): (u16, Vec<u8>) = if method == "HEAD" {
            if s.head_status == 200 {
                headers.push(("Content-Length".into(), s.head_len.unwrap_or(len).to_string()));
                (200, Vec::new())
            } else {
                headers.push(("Content-Length".into(), "0".into()));
                (s.head_status, Vec::new())
            }
        } else if let Some(code) = s.get_status {
            headers.push(("Content-Length".into(), "0".into()));
            (code, Vec::new())
        } else {
            let start = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.strip_suffix('-'))
                .and_then(|n| n.parse::<usize>().ok());
            let validator_ok = match (&if_range, &s.etag) {
                (None, _) => true,
                (Some(sent), Some(current)) => sent == current,
                (Some(_), None) => false,
            };
            match start {
                Some(n) if validator_ok && n >= len => {
                    headers.push(("Content-Range".into(), format!("bytes */{}", len)));
                    headers.push(("Content-Length".into(), "0".into()));
                    (416, Vec::new())
                }
                Some(n) if validator_ok => {
                    let from = if s.wrong_range_start { 0 } else { n };
                    headers.push(("Content-Range".into(), format!("bytes {}-{}/{}", from, len - 1, len)));
                    headers.push(("Content-Length".into(), (len - from).to_string()));
                    (206, s.body[from..].to_vec())
                }
                _ => {
                    headers.push(("Content-Length".into(), len.to_string()));
                    (200, s.body.clone())
                }
            }
        };
        let headers = if s.no_length && method == "GET" {
            headers.into_iter().filter(|(k, _)| k != "Content-Length").collect()
        } else {
            headers
        };
        (status, headers, body, if method == "GET" { s.stall_after } else { None })
    };

    state.lock().unwrap().log.push(Logged { method: method.clone(), range, if_range, status });

    let mut head = format!("HTTP/1.1 {} X\r\nConnection: close\r\n", status);
    for (k, v) in headers {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;

    match stall_after {
        Some(n) if n < body.len() => {
            stream.write_all(&body[..n])?;
            stream.flush()?;
            // Go silent without closing, like a dead connection behind a proxy.
            std::thread::sleep(Duration::from_secs(10));
        }
        _ => {
            for part in body.chunks(16 * 1024) {
                stream.write_all(part)?;
            }
        }
    }
    stream.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Job helpers
// ---------------------------------------------------------------------------------------

struct Harness {
    _dir: tempfile::TempDir,
    paths: AppPaths,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = test_paths(dir.path());
        Harness { _dir: dir, paths }
    }

    fn job(
        &self,
        model_id: &str,
        kind: &str,
        url: String,
        sha256: String,
        archive: Option<&str>,
    ) -> (DownloadJob, flume::Receiver<DownloadProgress>, Arc<AtomicBool>) {
        let (tx, rx) = flume::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let job = DownloadJob {
            job_id: "job-1".into(),
            model_id: model_id.into(),
            name: format!("Test {}", model_id),
            url,
            sha256,
            archive: archive.map(str::to_string),
            size_bytes: 1024,
            kind: kind.into(),
            paths: self.paths.clone(),
            tx,
            cancel: cancel.clone(),
        };
        (job, rx, cancel)
    }

    fn part(&self, id: &str) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.part", id))
    }

    fn validator(&self, id: &str) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.part.validator", id))
    }

    fn scratch(&self, id: &str) -> PathBuf {
        self.paths.cache_downloads.join(format!("{}.extract", id))
    }
}

fn states(rx: &flume::Receiver<DownloadProgress>) -> Vec<DownloadState> {
    rx.try_iter().map(|p| p.state).collect()
}

fn read_manifest(dir: &Path) -> ModelManifest {
    ModelManifest::read(dir).expect("manifest.json")
}

// ---------------------------------------------------------------------------------------
// End-to-end downloads
// ---------------------------------------------------------------------------------------

#[tokio::test]
async fn fresh_download_installs_file_and_manifest() {
    let body = pseudo_random(300_000, 1);
    let sha = sha_hex(&body);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    let (job, rx, _) = h.job("stt-fresh", "stt", server.url("ggml-test.bin"), sha.to_uppercase(), None);

    job.run().await.expect("download succeeds");

    let dir = h.paths.model_dir("stt", "stt-fresh");
    assert_eq!(std::fs::read(dir.join("ggml-test.bin")).unwrap(), body);
    let manifest = read_manifest(&dir);
    assert_eq!(manifest.id, "stt-fresh");
    assert_eq!(manifest.sha256, sha);
    assert_eq!(manifest.files, vec!["ggml-test.bin".to_string()]);
    assert_eq!(manifest.size_bytes, body.len() as u64);

    let states = states(&rx);
    assert_eq!(states.first(), Some(&DownloadState::Downloading));
    assert!(states.contains(&DownloadState::Verifying));
    assert_eq!(states.last(), Some(&DownloadState::Installed));

    assert!(!h.part("stt-fresh").exists());
    assert!(!h.validator("stt-fresh").exists());
    let gets = server.gets();
    assert_eq!(gets.len(), 1);
    assert_eq!(gets[0].range, None);
    assert_eq!(gets[0].status, 200);
}

#[tokio::test]
async fn sha_mismatch_fails_and_deletes_part() {
    let body = pseudo_random(50_000, 2);
    let server = TestServer::start(body, Some("\"v1\""));
    let h = Harness::new();
    let (job, rx, _) = h.job("stt-bad", "stt", server.url("m.bin"), "0".repeat(64), None);

    let err = job.run().await.unwrap_err();
    assert!(matches!(err, AppError::Download(ref m) if m.contains("SHA256 mismatch")), "{err}");

    let last = rx.try_iter().last().unwrap();
    assert_eq!(last.state, DownloadState::Failed);
    assert!(last.error.unwrap().contains("SHA256 mismatch"));
    assert!(!h.part("stt-bad").exists());
    assert!(!h.validator("stt-bad").exists());
    assert!(!h.paths.model_dir("stt", "stt-bad").exists());
}

#[tokio::test]
async fn resumes_from_existing_part_with_range_request() {
    let body = pseudo_random(200_000, 3);
    let half = 77_777;
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    std::fs::write(h.part("stt-resume"), &body[..half]).unwrap();
    std::fs::write(h.validator("stt-resume"), "\"v1\"").unwrap();
    let (job, rx, _) = h.job("stt-resume", "stt", server.url("m.bin"), sha_hex(&body), None);

    job.run().await.expect("resumed download succeeds");

    let gets = server.gets();
    assert_eq!(gets.len(), 1);
    assert_eq!(gets[0].range.as_deref(), Some(format!("bytes={}-", half).as_str()));
    assert_eq!(gets[0].if_range.as_deref(), Some("\"v1\""));
    assert_eq!(gets[0].status, 206);

    let dir = h.paths.model_dir("stt", "stt-resume");
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
    assert_eq!(read_manifest(&dir).sha256, sha_hex(&body));
    assert_eq!(states(&rx).last(), Some(&DownloadState::Installed));
}

#[tokio::test]
async fn resume_without_saved_validator_still_verifies() {
    // A .part left by an older build (no validator file): resume, and let the hash decide.
    let body = pseudo_random(120_000, 4);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    std::fs::write(h.part("stt-legacy"), &body[..1000]).unwrap();
    let (job, _rx, _) = h.job("stt-legacy", "stt", server.url("m.bin"), sha_hex(&body), None);

    job.run().await.unwrap();
    let gets = server.gets();
    assert_eq!(gets[0].status, 206);
    assert_eq!(gets[0].if_range.as_deref(), Some("\"v1\""));
}

#[tokio::test]
async fn file_changed_upstream_restarts_from_zero() {
    // The .part holds the start of an older version; HEAD now reports a new ETag.
    let old = pseudo_random(100_000, 5);
    let new = pseudo_random(90_000, 6);
    let server = TestServer::start(new.clone(), Some("\"v2\""));
    let h = Harness::new();
    std::fs::write(h.part("stt-changed"), &old[..40_000]).unwrap();
    std::fs::write(h.validator("stt-changed"), "\"v1\"").unwrap();
    let (job, _rx, _) = h.job("stt-changed", "stt", server.url("m.bin"), sha_hex(&new), None);

    job.run().await.expect("restarted download succeeds");

    let gets = server.gets();
    assert_eq!(gets.len(), 1);
    assert_eq!(gets[0].range, None, "must not resume a different version");
    assert_eq!(gets[0].status, 200);
    let dir = h.paths.model_dir("stt", "stt-changed");
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), new);
}

#[tokio::test]
async fn if_range_mismatch_without_head_restarts_from_zero() {
    // HEAD unsupported, so only If-Range can notice the change: the server answers 200.
    let old = pseudo_random(100_000, 7);
    let new = pseudo_random(100_000, 8);
    let server = TestServer::start(new.clone(), Some("\"v2\""));
    server.with(|s| s.head_status = 405);
    let h = Harness::new();
    std::fs::write(h.part("stt-ifrange"), &old[..30_000]).unwrap();
    std::fs::write(h.validator("stt-ifrange"), "\"v1\"").unwrap();
    let (job, _rx, _) = h.job("stt-ifrange", "stt", server.url("m.bin"), sha_hex(&new), None);

    job.run().await.expect("restarted download succeeds");

    let gets = server.gets();
    assert_eq!(gets.len(), 1);
    assert_eq!(gets[0].range.as_deref(), Some("bytes=30000-"));
    assert_eq!(gets[0].if_range.as_deref(), Some("\"v1\""));
    assert_eq!(gets[0].status, 200);
    let dir = h.paths.model_dir("stt", "stt-ifrange");
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), new);
}

#[tokio::test]
async fn range_not_satisfiable_with_complete_part_verifies() {
    let body = pseudo_random(64_000, 9);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| s.head_status = 405); // size unknown, so the downloader asks for the rest
    let h = Harness::new();
    std::fs::write(h.part("stt-416"), &body).unwrap();
    std::fs::write(h.validator("stt-416"), "\"v1\"").unwrap();
    let (job, rx, _) = h.job("stt-416", "stt", server.url("m.bin"), sha_hex(&body), None);

    job.run().await.expect("complete .part verifies");

    let gets = server.gets();
    assert_eq!(gets.len(), 1);
    assert_eq!(gets[0].status, 416);
    let dir = h.paths.model_dir("stt", "stt-416");
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
    assert_eq!(states(&rx).last(), Some(&DownloadState::Installed));
}

#[tokio::test]
async fn complete_part_with_known_size_skips_get() {
    let body = pseudo_random(64_000, 10);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    std::fs::write(h.part("stt-done"), &body).unwrap();
    std::fs::write(h.validator("stt-done"), "\"v1\"").unwrap();
    let (job, _rx, _) = h.job("stt-done", "stt", server.url("m.bin"), sha_hex(&body), None);

    job.run().await.unwrap();
    assert!(server.gets().is_empty(), "HEAD size matches the .part, nothing to fetch");
    assert!(h.paths.model_dir("stt", "stt-done").join("m.bin").is_file());
}

#[tokio::test]
async fn oversized_part_is_discarded() {
    let body = pseudo_random(10_000, 11);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    std::fs::write(h.part("stt-big"), pseudo_random(20_000, 12)).unwrap();
    let (job, _rx, _) = h.job("stt-big", "stt", server.url("m.bin"), sha_hex(&body), None);

    job.run().await.unwrap();
    let gets = server.gets();
    assert_eq!(gets[0].range, None);
    assert_eq!(gets[0].status, 200);
}

#[tokio::test]
async fn wrong_resume_position_fails_and_discards_part() {
    let body = pseudo_random(50_000, 13);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| s.wrong_range_start = true);
    let h = Harness::new();
    std::fs::write(h.part("stt-wrong"), &body[..1000]).unwrap();
    std::fs::write(h.validator("stt-wrong"), "\"v1\"").unwrap();
    let (job, _rx, _) = h.job("stt-wrong", "stt", server.url("m.bin"), sha_hex(&body), None);

    let err = job.run().await.unwrap_err();
    assert!(err.to_string().contains("wrong position"), "{err}");
    assert!(!h.part("stt-wrong").exists());
}

#[tokio::test]
async fn http_error_status_fails() {
    let server = TestServer::start(Vec::new(), None);
    server.with(|s| {
        s.head_status = 404;
        s.get_status = Some(404);
    });
    let h = Harness::new();
    let (job, rx, _) = h.job("stt-404", "stt", server.url("m.bin"), "a".repeat(64), None);

    let err = job.run().await.unwrap_err();
    assert!(err.to_string().contains("404"), "{err}");
    assert_eq!(states(&rx).last(), Some(&DownloadState::Failed));
    assert!(!h.paths.model_dir("stt", "stt-404").exists());
}

#[tokio::test]
async fn cancel_during_stalled_stream_is_prompt() {
    let body = pseudo_random(1_000_000, 14);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| s.stall_after = Some(32 * 1024));
    let h = Harness::new();
    let (job, rx, cancel) = h.job("stt-stall", "stt", server.url("m.bin"), sha_hex(&body), None);

    let task = tokio::spawn(job.run());
    // Wait until the body has started (and then stalled).
    let started = Instant::now();
    while server.gets().is_empty() {
        assert!(started.elapsed() < Duration::from_secs(10), "GET never arrived");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    let cancelled_at = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("cancel must not hang")
        .unwrap();
    let took = cancelled_at.elapsed();

    assert!(matches!(result, Err(AppError::Cancelled)), "{result:?}");
    assert!(took < Duration::from_secs(1), "cancel took {took:?}");
    assert_eq!(states(&rx).last(), Some(&DownloadState::Cancelled));
    assert!(!h.part("stt-stall").exists());
    assert!(!h.validator("stt-stall").exists());
    assert!(!h.paths.model_dir("stt", "stt-stall").exists());
}

#[tokio::test]
async fn archive_download_is_extracted_and_scratch_removed() {
    let archive = tar_bz2(&[
        Item::Dir("vits-test/"),
        Item::File("vits-test/model.onnx", b"onnx-bytes"),
        Item::File("vits-test/tokens.txt", b"a 1\nb 2\n"),
        Item::Dir("vits-test/espeak-ng-data/"),
        Item::File("vits-test/espeak-ng-data/phontab", b"phon"),
    ]);
    let sha = sha_hex(&archive);
    let server = TestServer::start(archive, Some("\"a1\""));
    let h = Harness::new();
    let (job, rx, _) = h.job("tts-archive", "tts", server.url("vits-test.tar.bz2"), sha.clone(), Some("tar.bz2"));

    job.run().await.expect("archive installs");

    let dir = h.paths.model_dir("tts", "tts-archive");
    assert_eq!(std::fs::read(dir.join("model.onnx")).unwrap(), b"onnx-bytes");
    assert!(dir.join("tokens.txt").is_file());
    assert!(dir.join("espeak-ng-data").join("phontab").is_file());
    let manifest = read_manifest(&dir);
    assert_eq!(manifest.sha256, sha);
    assert_eq!(
        manifest.files,
        vec!["espeak-ng-data/phontab".to_string(), "model.onnx".into(), "tokens.txt".into()]
    );
    assert!(states(&rx).contains(&DownloadState::Extracting));
    assert!(!h.scratch("tts-archive").exists());
    assert!(!h.part("tts-archive").exists());
    assert!(!h.validator("tts-archive").exists());
}

#[tokio::test]
async fn malicious_archive_download_fails_cleanly() {
    let archive = tar_bz2(&[Item::File("ok.txt", b"x"), Item::Raw("../evil.txt", EntryType::Regular, b"pwn", None)]);
    let server = TestServer::start(archive.clone(), Some("\"a1\""));
    let h = Harness::new();
    let (job, rx, _) = h.job("tts-evil", "tts", server.url("x.tar.bz2"), sha_hex(&archive), Some("tar.bz2"));

    let err = job.run().await.unwrap_err();
    assert!(err.to_string().contains("outside the model folder"), "{err}");
    assert_eq!(states(&rx).last(), Some(&DownloadState::Failed));
    assert!(!h.paths.model_dir("tts", "tts-evil").exists());
    assert!(!h.scratch("tts-evil").exists());
    assert!(!h.paths.cache_downloads.join("evil.txt").exists());
}

#[tokio::test]
async fn invalid_model_id_is_refused_before_any_request() {
    let server = TestServer::start(b"x".to_vec(), None);
    let h = Harness::new();
    for id in ["../escape", "C:evil", "..", ""] {
        let (job, rx, _) = h.job(id, "stt", server.url("m.bin"), sha_hex(b"x"), None);
        let err = job.run().await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{id:?}: {err}");
        assert_eq!(states(&rx).last(), Some(&DownloadState::Failed));
    }
    assert!(server.log().is_empty());
}

#[tokio::test]
async fn missing_checksum_follows_build_policy() {
    let body = pseudo_random(5_000, 15);
    let server = TestServer::start(body, Some("\"v1\""));
    let h = Harness::new();
    let (job, _rx, _) = h.job("stt-nosha", "stt", server.url("m.bin"), String::new(), None);

    let result = job.run().await;
    if cfg!(debug_assertions) {
        result.expect("debug builds install unverified files");
        assert!(h.paths.model_dir("stt", "stt-nosha").join("manifest.json").is_file());
    } else {
        assert!(result.is_err());
        assert!(server.log().is_empty(), "release builds refuse before downloading");
    }
}

#[tokio::test]
async fn body_larger_than_catalog_size_is_stopped_and_discarded() {
    // No length from HEAD or GET: only the catalog size (plus slack) bounds the stream.
    let body = pseudo_random(3 * 1024 * 1024, 17);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| {
        s.head_status = 405;
        s.no_length = true;
    });
    let h = Harness::new();
    let (mut job, rx, _) = h.job("stt-huge", "stt", server.url("m.bin"), sha_hex(&body), None);
    job.size_bytes = 100_000; // limit: 100 000 + 1 MB, well under the 3 MB sent

    let err = job.run().await.unwrap_err();
    assert!(matches!(err, AppError::Download(ref m) if m.contains("more data than expected")), "{err}");
    assert_eq!(states(&rx).last(), Some(&DownloadState::Failed));
    assert!(!h.part("stt-huge").exists(), "the oversized partial file must be deleted");
    assert!(!h.validator("stt-huge").exists());
    assert!(!h.paths.model_dir("stt", "stt-huge").exists());
}

#[tokio::test]
async fn body_larger_than_head_size_is_stopped() {
    // HEAD declares fewer bytes than the GET then streams (without a length of its own).
    let body = pseudo_random(200_000, 18);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| {
        s.head_len = Some(50_000);
        s.no_length = true;
    });
    let h = Harness::new();
    let (job, _rx, _) = h.job("stt-liar", "stt", server.url("m.bin"), sha_hex(&body), None);

    let err = job.run().await.unwrap_err();
    assert!(err.to_string().contains("more data than expected"), "{err}");
    assert!(!h.part("stt-liar").exists());
}

#[tokio::test]
async fn body_without_length_within_catalog_size_installs() {
    let body = pseudo_random(80_000, 19);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    server.with(|s| {
        s.head_status = 405;
        s.no_length = true;
    });
    let h = Harness::new();
    let (mut job, _rx, _) = h.job("stt-nolen", "stt", server.url("m.bin"), sha_hex(&body), None);
    job.size_bytes = body.len() as u64;

    job.run().await.expect("a body of the catalog size installs");
    let dir = h.paths.model_dir("stt", "stt-nolen");
    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
}

#[tokio::test]
async fn torn_manifest_does_not_block_a_fresh_download() {
    let body = pseudo_random(40_000, 20);
    let sha = sha_hex(&body);
    let server = TestServer::start(body.clone(), Some("\"v1\""));
    let h = Harness::new();
    // A crash mid-install left the model file and half a manifest.
    let dir = h.paths.model_dir("stt", "stt-torn");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.bin"), b"stale").unwrap();
    std::fs::write(dir.join("manifest.json"), "{\"id\":\"stt-to").unwrap();
    assert!(ModelManifest::read(&dir).is_none());

    let (job, _rx, _) = h.job("stt-torn", "stt", server.url("m.bin"), sha.clone(), None);
    job.run().await.expect("re-download over a torn install succeeds");

    assert_eq!(std::fs::read(dir.join("m.bin")).unwrap(), body);
    let manifest = read_manifest(&dir);
    assert_eq!(manifest.sha256, sha);
    assert_eq!(manifest.files, vec!["m.bin".to_string()]);
    // The atomic write leaves no temp file behind.
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn write_manifest_replaces_a_torn_one() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("model.bin"), b"weights").unwrap();
    std::fs::write(dir.path().join("manifest.json"), "{ torn").unwrap();
    let manifest = ModelManifest {
        id: "m".into(),
        version: "1.0".into(),
        sha256: "a".repeat(64),
        installed_at: 1,
        size_bytes: 7,
        files: vec!["model.bin".into()],
    };
    write_manifest(dir.path(), &manifest).unwrap();
    assert_eq!(read_manifest(dir.path()).files, manifest.files);
}

// ---------------------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------------------

#[test]
fn download_size_limit() {
    const MB: u64 = 1024 * 1024;
    // Catalog size plus the larger of 1% and 1 MB.
    assert_eq!(max_download_bytes(10 * MB, 0), 11 * MB);
    assert_eq!(max_download_bytes(1000 * MB, 0), 1010 * MB);
    // A smaller server size wins; a larger one does not raise the cap.
    assert_eq!(max_download_bytes(1000 * MB, 900 * MB), 900 * MB);
    assert_eq!(max_download_bytes(10 * MB, 50 * MB), 11 * MB);
    assert_eq!(max_download_bytes(0, 5 * MB), 5 * MB);
    // Nothing known: the hard ceiling.
    assert_eq!(max_download_bytes(0, 0), MAX_DOWNLOAD_BYTES);
    assert_eq!(MAX_DOWNLOAD_BYTES, 8 * 1024 * MB);
    assert_eq!(max_download_bytes(u64::MAX, 0), u64::MAX);
}

#[test]
fn checksum_policy() {
    let good = "AB".repeat(32);
    assert_eq!(expected_checksum(&good, false).unwrap(), Some("ab".repeat(32)));
    assert_eq!(expected_checksum(&format!("  {}\n", good), false).unwrap(), Some("ab".repeat(32)));
    for bad in ["", "abc", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
        assert!(matches!(expected_checksum(bad, false), Err(AppError::Download(_))), "{bad:?}");
        assert_eq!(expected_checksum(bad, true).unwrap(), None, "{bad:?}");
    }
    assert_eq!(ALLOW_UNVERIFIED, cfg!(debug_assertions));
}

#[test]
fn disk_space_arithmetic() {
    const MB: u64 = 1024 * 1024;
    assert_eq!(download_space_needed(100 * MB, 0, false), 100 * MB + DISK_MARGIN);
    assert_eq!(download_space_needed(100 * MB, 40 * MB, false), 60 * MB + DISK_MARGIN);
    // Archives also need room for the extracted files (about 2.5x).
    assert_eq!(download_space_needed(100 * MB, 0, true), 350 * MB + DISK_MARGIN);
    assert_eq!(download_space_needed(100 * MB, 100 * MB, true), 250 * MB + DISK_MARGIN);
    // A .part larger than the file never underflows; huge sizes never overflow.
    assert_eq!(download_space_needed(10, 20, false), DISK_MARGIN);
    assert_eq!(download_space_needed(u64::MAX, 0, true), u64::MAX);
}

#[test]
fn disk_space_check() {
    let mount = Path::new("/mnt/models");
    assert!(check_space("Whisper", 1000, mount, 1000).is_ok());
    assert!(check_space("Whisper", 1000, mount, 5000).is_ok());
    let err = check_space("Whisper Tiny", 3 * 1024 * 1024 * 1024, mount, 512 * 1024 * 1024).unwrap_err();
    let AppError::DiskFull(msg) = err else { panic!("expected DiskFull") };
    assert!(msg.contains("Whisper Tiny"), "{msg}");
    assert!(msg.contains("3.0 GB"), "{msg}");
    assert!(msg.contains("512 MB"), "{msg}");
    assert!(msg.contains(&mount.display().to_string()), "{msg}");
}

#[test]
fn free_space_lookup_works_for_a_real_dir() {
    let dir = tempfile::tempdir().unwrap();
    // The temp dir's disk is known to sysinfo on every CI platform we use.
    assert!(free_space_for(dir.path()).is_some());
    assert!(ensure_free_space(dir.path(), 1, "x").is_ok());
    assert!(matches!(ensure_free_space(dir.path(), u64::MAX, "x"), Err(AppError::DiskFull(_))));
    // An unknown disk is not an error.
    assert!(ensure_free_space(&dir.path().join("missing"), u64::MAX, "x").is_ok());
}

#[test]
fn disk_full_classification() {
    use std::io::{Error, ErrorKind};
    assert!(is_disk_full(&Error::from(ErrorKind::StorageFull)));
    #[cfg(windows)]
    {
        assert!(is_disk_full(&Error::from_raw_os_error(112)));
        assert!(is_disk_full(&Error::from_raw_os_error(39)));
        assert!(!is_disk_full(&Error::from_raw_os_error(5)));
    }
    #[cfg(unix)]
    {
        assert!(is_disk_full(&Error::from_raw_os_error(28)));
        assert!(!is_disk_full(&Error::from_raw_os_error(13)));
    }
    assert!(!is_disk_full(&Error::from(ErrorKind::NotFound)));
    assert!(!is_disk_full(&Error::other("disk full"))); // text is not a signal
}

#[test]
fn plain_http_only_for_loopback_in_tests() {
    for url in ["http://127.0.0.1:8080/a", "http://localhost/a", "http://[::1]:9/a", "http://127.1.2.3/a"] {
        assert!(is_loopback_http(url), "{url}");
        assert!(plain_http_allowed(url), "{url}");
    }
    for url in [
        "https://127.0.0.1/a",
        "http://example.com/a",
        "http://127.0.0.1.example.com/a",
        "http://10.0.0.1/a",
        "http://[::2]/a",
        "not a url",
    ] {
        assert!(!is_loopback_http(url), "{url}");
        assert!(!plain_http_allowed(url), "{url}");
    }
}

#[tokio::test]
async fn client_refuses_plain_http_to_other_hosts() {
    // https_only rejects the request before any connection is attempted.
    let client = http_client("http://example.invalid/model.bin").unwrap();
    let err = client.get("http://example.invalid/model.bin").send().await.unwrap_err();
    assert!(err.is_builder() || err.to_string().to_lowercase().contains("https"), "{err}");
}

#[test]
fn validator_prefers_strong_etag() {
    use reqwest::header::{HeaderMap, HeaderValue, ETAG, LAST_MODIFIED};
    let mut h = HeaderMap::new();
    assert_eq!(validator_from(&h), None);
    h.insert(LAST_MODIFIED, HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"));
    assert_eq!(validator_from(&h).as_deref(), Some("Wed, 21 Oct 2015 07:28:00 GMT"));
    h.insert(ETAG, HeaderValue::from_static("W/\"weak\""));
    assert_eq!(validator_from(&h).as_deref(), Some("Wed, 21 Oct 2015 07:28:00 GMT"));
    h.insert(ETAG, HeaderValue::from_static("\"strong\""));
    assert_eq!(validator_from(&h).as_deref(), Some("\"strong\""));
}

#[test]
fn content_range_parsing() {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_RANGE};
    let mut h = HeaderMap::new();
    assert_eq!(content_range_start(&h), None);
    h.insert(CONTENT_RANGE, HeaderValue::from_static("bytes 100-199/200"));
    assert_eq!(content_range_start(&h), Some(100));
    h.insert(CONTENT_RANGE, HeaderValue::from_static("bytes */200"));
    assert_eq!(content_range_start(&h), None);
}

#[test]
fn file_name_from_url() {
    assert_eq!(url_file_name("https://h/a/ggml-base.en.bin"), Some("ggml-base.en.bin"));
    assert_eq!(url_file_name("https://h/a/model.bin?download=true#x"), Some("model.bin"));
    assert_eq!(url_file_name("https://h/a/"), None);
    assert_eq!(url_file_name("https://h/a/..%5C..%5Cevil"), None);
    assert_eq!(url_file_name("https://h/a/.."), None);
    assert_eq!(url_file_name("https://h/a/C:evil"), None);
}

#[test]
fn download_manager_dedupes_and_cancels() {
    let m = DownloadManager::default();
    let flag = m.register("j1", "model-a").unwrap();
    assert!(m.register("j2", "model-a").is_err());
    assert!(m.is_downloading("model-a"));
    assert!(!m.cancel("nope"));
    assert!(m.cancel("model-a"));
    assert!(flag.load(Ordering::Relaxed));
    m.finish("j1");
    assert!(!m.is_downloading("model-a"));
    let flag = m.register("j3", "model-a").unwrap();
    assert!(m.cancel("j3"));
    assert!(flag.load(Ordering::Relaxed));
}

// ---------------------------------------------------------------------------------------
// Archive extraction
// ---------------------------------------------------------------------------------------

enum Item<'a> {
    Dir(&'a str),
    File(&'a str, &'a [u8]),
    /// Written with a raw header, bypassing the tar builder's own path checks.
    Raw(&'a str, EntryType, &'a [u8], Option<&'a str>),
}

fn raw_header(path: &str, kind: EntryType, size: u64, link: Option<&str>) -> Header {
    let mut header = Header::new_gnu();
    {
        let gnu = header.as_gnu_mut().unwrap();
        gnu.name[..path.len()].copy_from_slice(path.as_bytes());
        if let Some(link) = link {
            gnu.linkname[..link.len()].copy_from_slice(link.as_bytes());
        }
    }
    header.set_entry_type(kind);
    header.set_size(size);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    header
}

fn tar_bytes(items: &[Item]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for item in items {
        match item {
            Item::Dir(path) => {
                let mut h = Header::new_gnu();
                h.set_entry_type(EntryType::Directory);
                h.set_size(0);
                h.set_mode(0o755);
                builder.append_data(&mut h, path, std::io::empty()).unwrap();
            }
            Item::File(path, data) => {
                let mut h = Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                builder.append_data(&mut h, path, *data).unwrap();
            }
            Item::Raw(path, kind, data, link) => {
                let h = raw_header(path, *kind, data.len() as u64, *link);
                builder.append(&h, *data).unwrap();
            }
        }
    }
    builder.into_inner().unwrap()
}

fn tar_bz2(items: &[Item]) -> Vec<u8> {
    bz2(&tar_bytes(items))
}

fn bz2(data: &[u8]) -> Vec<u8> {
    let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::best());
    enc.write_all(data).unwrap();
    enc.finish().unwrap()
}

struct Extract {
    dir: tempfile::TempDir,
}

impl Extract {
    fn new() -> Self {
        Extract { dir: tempfile::tempdir().unwrap() }
    }
    fn archive(&self) -> PathBuf {
        self.dir.path().join("dl").join("m.part")
    }
    fn dest(&self) -> PathBuf {
        self.dir.path().join("models").join("m")
    }
    fn scratch(&self) -> PathBuf {
        self.dir.path().join("dl").join("m.extract")
    }
    fn run(&self, bytes: &[u8], format: &str) -> Result<()> {
        std::fs::create_dir_all(self.archive().parent().unwrap()).unwrap();
        std::fs::write(self.archive(), bytes).unwrap();
        extract_archive(&self.archive(), &self.dest(), &self.scratch(), format)
    }
    fn assert_nothing_left(&self) {
        assert!(!self.dest().exists(), "dest must not appear on failure");
        assert!(!self.scratch().exists(), "scratch must be cleaned up");
    }
}

#[test]
fn extracts_normal_archive_and_flattens_single_root() {
    let x = Extract::new();
    let bytes = tar_bz2(&[
        Item::Dir("kokoro/"),
        Item::File("kokoro/model.onnx", b"m"),
        Item::File("kokoro/voices.bin", b"v"),
        Item::Dir("kokoro/espeak-ng-data/"),
        Item::File("kokoro/espeak-ng-data/en_dict", b"d"),
        Item::File("./kokoro/tokens.txt", b"t"),
    ]);
    x.run(&bytes, "tar.bz2").unwrap();
    let dest = x.dest();
    assert_eq!(std::fs::read(dest.join("model.onnx")).unwrap(), b"m");
    assert!(dest.join("voices.bin").is_file());
    assert!(dest.join("tokens.txt").is_file());
    assert!(dest.join("espeak-ng-data").join("en_dict").is_file());
    assert!(!x.scratch().exists());
}

#[test]
fn extracts_multi_root_archive_as_is() {
    let x = Extract::new();
    let bytes = tar_bytes(&[Item::File("a.txt", b"a"), Item::File("b/c.txt", b"c")]);
    x.run(&bytes, "tar").unwrap();
    assert!(x.dest().join("a.txt").is_file());
    assert!(x.dest().join("b").join("c.txt").is_file());
    assert!(!x.scratch().exists());
}

#[test]
fn rejects_parent_dir_entry() {
    let x = Extract::new();
    let bytes = tar_bz2(&[
        Item::File("m/ok.txt", b"x"),
        Item::Raw("m/../../evil.txt", EntryType::Regular, b"pwn", None),
    ]);
    let err = x.run(&bytes, "tar.bz2").unwrap_err();
    assert!(err.to_string().contains("outside the model folder"), "{err}");
    x.assert_nothing_left();
    assert!(!x.dir.path().join("evil.txt").exists());
    assert!(!x.dir.path().join("dl").join("evil.txt").exists());
}

#[test]
fn rejects_absolute_entry() {
    let x = Extract::new();
    let evil = x.dir.path().join("abs-evil.txt");
    let abs = if cfg!(windows) {
        evil.to_string_lossy().replace('\\', "/")
    } else {
        evil.to_string_lossy().to_string()
    };
    for path in [abs.as_str(), "/tmp/talkr-abs-evil.txt"] {
        let bytes = tar_bz2(&[Item::Raw(path, EntryType::Regular, b"pwn", None)]);
        let err = x.run(&bytes, "tar.bz2").unwrap_err();
        assert!(err.to_string().contains("Refusing to extract"), "{path}: {err}");
        x.assert_nothing_left();
    }
    assert!(!evil.exists());
}

#[test]
fn rejects_symlink_entry() {
    let x = Extract::new();
    let bytes = tar_bz2(&[
        Item::File("m/ok.txt", b"x"),
        Item::Raw("m/link", EntryType::Symlink, b"", Some("/etc/passwd")),
    ]);
    let err = x.run(&bytes, "tar.bz2").unwrap_err();
    assert!(err.to_string().contains("Symlink"), "{err}");
    x.assert_nothing_left();
}

#[test]
fn rejects_hardlink_entry() {
    let x = Extract::new();
    let bytes = tar_bz2(&[
        Item::File("m/ok.txt", b"x"),
        Item::Raw("m/hard", EntryType::Link, b"", Some("m/ok.txt")),
    ]);
    let err = x.run(&bytes, "tar.bz2").unwrap_err();
    assert!(err.to_string().contains("Link"), "{err}");
    x.assert_nothing_left();
}

#[test]
fn rejects_device_and_fifo_entries() {
    for kind in [EntryType::Char, EntryType::Block, EntryType::Fifo] {
        let x = Extract::new();
        let bytes = tar_bytes(&[Item::Raw("m/dev", kind, b"", None)]);
        assert!(x.run(&bytes, "tar").is_err(), "{kind:?}");
        x.assert_nothing_left();
    }
}

#[test]
fn rejects_colon_in_entry_name() {
    let x = Extract::new();
    let bytes = tar_bytes(&[Item::Raw("m/file.txt:stream", EntryType::Regular, b"x", None)]);
    assert!(x.run(&bytes, "tar").is_err());
    x.assert_nothing_left();
}

#[test]
fn rejects_decompression_bomb() {
    let x = Extract::new();
    // 4 MB of zeros compresses to a few hundred bytes: far over 10x the archive size.
    let zeros = vec![0u8; 4 * 1024 * 1024];
    let bytes = tar_bz2(&[Item::File("m/bomb.bin", &zeros)]);
    assert!((bytes.len() as u64) * MAX_EXPANSION_RATIO < zeros.len() as u64);
    let err = x.run(&bytes, "tar.bz2").unwrap_err();
    assert!(err.to_string().contains("decompression bomb"), "{err}");
    x.assert_nothing_left();
}

#[test]
fn bomb_cap_counts_all_entries() {
    // Each file alone is under the cap; together they are not.
    let x = Extract::new();
    let data = pseudo_random(4000, 16);
    let bytes = tar_bytes(&[Item::File("a", &data), Item::File("b", &data), Item::File("c", &data)]);
    let dir = tempfile::tempdir().unwrap();
    let limit = 10_000;
    let err = unpack_checked(tar::Archive::new(&bytes[..]), dir.path(), limit).unwrap_err();
    assert!(err.to_string().contains("decompression bomb"), "{err}");
    assert!(!dir.path().join("c").exists(), "the entry over the cap must not be written");
    assert!(x.run(&bytes, "tar").is_ok(), "an uncompressed tar is well under 10x its size");
}

#[test]
fn extraction_limit_is_capped() {
    assert_eq!(extraction_limit(0), 0);
    assert_eq!(extraction_limit(100), 1000);
    assert_eq!(extraction_limit(500 * 1024 * 1024), 5000 * 1024 * 1024);
    assert_eq!(extraction_limit(2 * 1024 * 1024 * 1024), MAX_EXTRACTED_BYTES);
    assert_eq!(extraction_limit(u64::MAX), MAX_EXTRACTED_BYTES);
}

#[test]
fn rejects_unknown_format_and_corrupt_archive() {
    let x = Extract::new();
    assert!(x.run(b"whatever", "zip").unwrap_err().to_string().contains("Unsupported"));
    x.assert_nothing_left();
    assert!(x.run(b"this is not bzip2 data at all", "tar.bz2").is_err());
    x.assert_nothing_left();
}

#[test]
fn entry_path_rules() {
    assert!(check_entry_path(Path::new("a/b.txt")).is_ok());
    assert!(check_entry_path(Path::new("./a")).is_ok());
    for bad in ["", ".", "..", "a/../../b", "/abs", "a/b:c"] {
        assert!(check_entry_path(Path::new(bad)).is_err(), "{bad:?}");
    }
    #[cfg(windows)]
    for bad in [r"C:\x", r"C:x", r"\\server\share\x", r"a\..\..\b"] {
        assert!(check_entry_path(Path::new(bad)).is_err(), "{bad:?}");
    }
}
