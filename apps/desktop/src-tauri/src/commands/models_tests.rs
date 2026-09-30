//! Tests for `commands/models.rs`: id handling, `locate_model` and local model import.

use super::*;
use crate::downloader::tests::test_paths;

struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    paths: AppPaths,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let paths = test_paths(&root.join("app"));
        std::fs::create_dir_all(root.join("src")).unwrap();
        Env { _dir: dir, root, paths }
    }

    /// A path under the (separate) source area.
    fn src(&self, rel: &str) -> PathBuf {
        self.root.join("src").join(rel)
    }

    fn write(&self, rel: &str, data: &[u8]) -> PathBuf {
        let path = self.src(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, data).unwrap();
        path
    }

    fn mkdir(&self, rel: &str) -> PathBuf {
        let path = self.src(rel);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn kokoro(&self, name: &str) -> PathBuf {
        self.write(&format!("{name}/model.onnx"), b"onnx");
        self.write(&format!("{name}/voices.bin"), &[0u8; 64]);
        self.write(&format!("{name}/tokens.txt"), b"a 1\n");
        self.write(&format!("{name}/espeak-ng-data/phontab"), b"p");
        self.src(name)
    }

    fn piper(&self, name: &str) -> PathBuf {
        self.write(&format!("{name}/en_US-test-medium.onnx"), b"onnx");
        self.write(&format!("{name}/en_US-test-medium.onnx.json"), b"{}");
        self.write(&format!("{name}/tokens.txt"), b"a 1\n");
        self.write(&format!("{name}/espeak-ng-data/phontab"), b"p");
        self.src(name)
    }

    /// No scratch dirs left behind in the downloads cache.
    fn assert_no_scratch(&self) {
        let leftovers: Vec<_> = std::fs::read_dir(&self.paths.cache_downloads)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    fn installed_ids(&self, kind: ModelKind) -> Vec<String> {
        let root = self.paths.model_dir(kind.as_str(), "x");
        let root = root.parent().unwrap();
        let mut ids: Vec<String> = std::fs::read_dir(root)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        ids.sort();
        ids
    }
}

fn ggml(payload: &[u8]) -> Vec<u8> {
    let mut data = GGML_MAGIC.to_vec();
    data.extend_from_slice(payload);
    data
}

fn validation_msg(result: Result<InstalledModel>) -> String {
    match result {
        Err(AppError::Validation(msg)) => msg,
        Err(other) => panic!("expected a validation error, got {other}"),
        Ok(m) => panic!("expected a validation error, got installed {}", m.id),
    }
}

// ----- ids ---------------------------------------------------------------------------------

#[test]
fn ggml_magic_matches_whisper_cpp() {
    // GGML_FILE_MAGIC 0x67676d6c written as a little-endian u32.
    assert_eq!(GGML_MAGIC, [0x6c, 0x6d, 0x67, 0x67]);
    assert_eq!(&GGML_MAGIC, b"lmgg");
}

#[test]
fn model_id_from_file_names() {
    assert_eq!(model_id_from_name("ggml-base.en").unwrap(), "ggml-base.en");
    assert_eq!(model_id_from_name("My Model (v2)").unwrap(), "My-Model--v2");
    assert_eq!(model_id_from_name("..hidden").unwrap(), "hidden");
    assert_eq!(model_id_from_name("C:evil").unwrap(), "C-evil");
    assert_eq!(model_id_from_name("caf\u{e9}").unwrap(), "caf");
    assert_eq!(model_id_from_name(&"a".repeat(300)).unwrap().len(), MAX_MODEL_ID_LEN);
    for bad in ["", ".", "..", "---", "(((", "\u{1F600}", "._-", "\u{0}"] {
        assert!(model_id_from_name(bad).is_err(), "{bad:?}");
    }
    for name in ["x y", "../../evil", "a\\b", "c:d", "a\u{0}b"] {
        let id = model_id_from_name(name).unwrap();
        assert!(is_valid_model_id(&id), "{name:?} -> {id:?}");
    }
}

#[test]
fn locate_model_rejects_unsafe_ids() {
    let env = Env::new();
    // A manifest reachable through traversal: models/stt/../evil == models/evil.
    let evil = env.paths.models.join("evil");
    std::fs::create_dir_all(&evil).unwrap();
    std::fs::write(evil.join("manifest.json"), "{}").unwrap();
    assert!(env.paths.model_dir("stt", "../evil").join("manifest.json").exists());

    for id in ["../evil", "..", ".", "", "a/b", "a\\b", "C:evil"] {
        assert!(locate_model(&env.paths, id).is_none(), "{id:?}");
    }
}

#[test]
fn locate_model_finds_catalog_and_imported_models() {
    let env = Env::new();
    // Catalog models resolve (installed or not) to their catalog kind.
    let (kind, dir) = locate_model(&env.paths, "whisper-tiny-en").unwrap();
    assert_eq!(kind, ModelKind::Stt);
    assert_eq!(dir, env.paths.model_dir("stt", "whisper-tiny-en"));
    // Imported models only once they have a manifest.
    assert!(locate_model(&env.paths, "my-voice").is_none());
    let src = env.piper("my-voice");
    import_model(&env.paths, &src, ModelKind::Tts).unwrap();
    assert_eq!(locate_model(&env.paths, "my-voice").unwrap().0, ModelKind::Tts);
}

// ----- STT import --------------------------------------------------------------------------

#[test]
fn imports_ggml_bin_file() {
    let env = Env::new();
    let src = env.write("ggml-custom.en.bin", &ggml(&[7u8; 1000]));

    let model = import_model(&env.paths, &src, ModelKind::Stt).unwrap();

    assert_eq!(model.id, "ggml-custom.en");
    assert_eq!(model.kind, ModelKind::Stt);
    assert_eq!(model.engine, "whisper");
    let dir = env.paths.model_dir("stt", "ggml-custom.en");
    assert_eq!(std::fs::read(dir.join("ggml-custom.en.bin")).unwrap(), ggml(&[7u8; 1000]));
    let manifest = ModelManifest::read(&dir).unwrap();
    assert_eq!(manifest.version, "local");
    assert_eq!(manifest.files, vec!["ggml-custom.en.bin".to_string()]);
    assert_eq!(manifest.size_bytes, 1004);
    assert!(src.exists(), "the source is copied, not moved");
    env.assert_no_scratch();
}

#[test]
fn imports_stt_folder_with_ggml_bin() {
    let env = Env::new();
    env.write("whisper-folder/ggml-model.bin", &ggml(b"weights"));
    env.write("whisper-folder/README.md", b"readme");
    let model = import_model(&env.paths, &env.src("whisper-folder"), ModelKind::Stt).unwrap();
    let dir = env.paths.model_dir("stt", &model.id);
    assert!(dir.join("ggml-model.bin").is_file());
    assert!(dir.join("README.md").is_file());
}

#[test]
fn rejects_gguf_file() {
    let env = Env::new();
    let src = env.write("model-gguf.bin", b"GGUF\x03\x00\x00\x00rest");
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Stt));
    assert!(msg.contains("GGUF"), "{msg}");
    assert!(env.installed_ids(ModelKind::Stt).is_empty());
    env.assert_no_scratch();
}

#[test]
fn rejects_non_ggml_bin() {
    let env = Env::new();
    for (name, data) in [
        ("random.bin", b"\x00\x01\x02\x03 not a model".as_slice()),
        ("big-endian.bin", b"ggml plus data".as_slice()),
        ("tiny.bin", b"lm".as_slice()),
        ("empty.bin", b"".as_slice()),
    ] {
        let src = env.write(name, data);
        let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Stt));
        assert!(msg.contains("GGML"), "{name}: {msg}");
    }
    assert!(env.installed_ids(ModelKind::Stt).is_empty());
    env.assert_no_scratch();
}

#[test]
fn rejects_stt_file_with_wrong_extension() {
    let env = Env::new();
    let src = env.write("model.onnx", &ggml(b"x"));
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Stt));
    assert!(msg.contains(".bin"), "{msg}");
}

#[test]
fn rejects_stt_folder_without_valid_bin() {
    let env = Env::new();
    env.write("no-bin/readme.txt", b"x");
    assert!(validation_msg(import_model(&env.paths, &env.src("no-bin"), ModelKind::Stt)).contains(".bin"));
    // The engine loads the first .bin by name, so that is the one that must be GGML.
    env.write("first-bad/a.bin", b"junkjunk");
    env.write("first-bad/b.bin", &ggml(b"x"));
    assert!(validation_msg(import_model(&env.paths, &env.src("first-bad"), ModelKind::Stt)).contains("GGML"));
    assert!(env.installed_ids(ModelKind::Stt).is_empty());
}

// ----- TTS import --------------------------------------------------------------------------

#[test]
fn imports_kokoro_folder() {
    let env = Env::new();
    let src = env.kokoro("kokoro-custom");
    let model = import_model(&env.paths, &src, ModelKind::Tts).unwrap();
    assert_eq!(model.id, "kokoro-custom");
    assert_eq!(model.engine, "sherpa-onnx");
    let dir = env.paths.model_dir("tts", "kokoro-custom");
    let manifest = ModelManifest::read(&dir).unwrap();
    assert_eq!(
        manifest.files,
        vec!["espeak-ng-data/phontab".to_string(), "model.onnx".into(), "tokens.txt".into(), "voices.bin".into()]
    );
    env.assert_no_scratch();
}

#[test]
fn imports_piper_folder() {
    let env = Env::new();
    let src = env.piper("piper custom voice");
    let model = import_model(&env.paths, &src, ModelKind::Tts).unwrap();
    assert_eq!(model.id, "piper-custom-voice");
    let dir = env.paths.model_dir("tts", "piper-custom-voice");
    assert!(dir.join("en_US-test-medium.onnx").is_file());
    assert!(dir.join("espeak-ng-data").join("phontab").is_file());
}

#[test]
fn rejects_incomplete_kokoro_folder() {
    let env = Env::new();
    let src = env.kokoro("kokoro-broken");
    std::fs::rename(src.join("model.onnx"), src.join("other.onnx")).unwrap();
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Tts));
    assert!(msg.contains("Kokoro") && msg.contains("model.onnx"), "{msg}");
    assert!(env.installed_ids(ModelKind::Tts).is_empty());
    env.assert_no_scratch();
}

#[test]
fn rejects_incomplete_piper_folders() {
    let env = Env::new();

    let src = env.piper("no-tokens");
    std::fs::remove_file(src.join("tokens.txt")).unwrap();
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains("tokens.txt"));

    let src = env.piper("no-espeak");
    std::fs::remove_dir_all(src.join("espeak-ng-data")).unwrap();
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains("espeak-ng-data"));

    let src = env.piper("espeak-is-a-file");
    std::fs::remove_dir_all(src.join("espeak-ng-data")).unwrap();
    std::fs::write(src.join("espeak-ng-data"), b"x").unwrap();
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains("espeak-ng-data"));

    let src = env.piper("no-onnx");
    std::fs::remove_file(src.join("en_US-test-medium.onnx")).unwrap();
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains(".onnx"));

    let empty = env.mkdir("empty");
    let msg = validation_msg(import_model(&env.paths, &empty, ModelKind::Tts));
    assert!(msg.contains("tokens.txt") && msg.contains("espeak-ng-data") && msg.contains(".onnx"), "{msg}");

    assert!(env.installed_ids(ModelKind::Tts).is_empty());
    env.assert_no_scratch();
}

#[test]
fn rejects_tts_file() {
    let env = Env::new();
    let src = env.write("voice.onnx", b"onnx");
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains("folder"));
}

// ----- general import rules ----------------------------------------------------------------

#[test]
fn missing_source_is_not_found() {
    let env = Env::new();
    let result = import_model(&env.paths, &env.src("does-not-exist"), ModelKind::Tts);
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[test]
fn refuses_catalog_id_conflict() {
    let env = Env::new();
    let src = env.write("whisper-tiny-en.bin", &ggml(b"x"));
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Stt));
    assert!(msg.contains("catalog"), "{msg}");
}

#[test]
fn refuses_duplicate_import_across_kinds() {
    let env = Env::new();
    let src = env.piper("dup");
    import_model(&env.paths, &src, ModelKind::Tts).unwrap();
    assert!(validation_msg(import_model(&env.paths, &src, ModelKind::Tts)).contains("already installed"));
    // Same id as an STT model would make locate_model ambiguous.
    let bin = env.write("stt/dup.bin", &ggml(b"x"));
    assert!(validation_msg(import_model(&env.paths, &bin, ModelKind::Stt)).contains("already installed"));
}

#[test]
fn replaces_leftovers_of_an_interrupted_import() {
    let env = Env::new();
    let junk = env.paths.model_dir("tts", "retry");
    std::fs::create_dir_all(&junk).unwrap();
    std::fs::write(junk.join("half-copied.onnx"), b"x").unwrap();
    let src = env.piper("retry");
    import_model(&env.paths, &src, ModelKind::Tts).unwrap();
    assert!(!junk.join("half-copied.onnx").exists());
    assert!(junk.join("manifest.json").is_file());
}

#[test]
fn copy_failure_leaves_nothing_behind() {
    let env = Env::new();
    let src = env.piper("will-fail");
    // Make the final move fail: the destination's parent is a file, not a directory.
    let blocker = env.root.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let mut paths = env.paths.clone();
    paths.models_tts = blocker.join("tts");
    assert!(import_model(&paths, &src, ModelKind::Tts).is_err());
    env.assert_no_scratch();
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path) -> bool {
    // Needs Developer Mode or admin rights; tests skip when unavailable.
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    } else {
        std::os::windows::fs::symlink_file(target, link).is_ok()
    }
}

#[test]
fn refuses_symlinks_inside_the_folder() {
    let env = Env::new();
    let secret = env.write("outside/secret.txt", b"secret");
    let src = env.piper("linked");
    if !make_symlink(&secret, &src.join("espeak-ng-data").join("secret.txt")) {
        eprintln!("skipping: cannot create symlinks here");
        return;
    }
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Tts));
    assert!(msg.contains("link"), "{msg}");
    assert!(env.installed_ids(ModelKind::Tts).is_empty());
    env.assert_no_scratch();
}

#[test]
fn refuses_symlinked_required_files() {
    let env = Env::new();
    let real = env.write("outside/tokens.txt", b"a 1\n");
    let src = env.piper("linked-tokens");
    std::fs::remove_file(src.join("tokens.txt")).unwrap();
    if !make_symlink(&real, &src.join("tokens.txt")) {
        eprintln!("skipping: cannot create symlinks here");
        return;
    }
    let msg = validation_msg(import_model(&env.paths, &src, ModelKind::Tts));
    assert!(msg.contains("tokens.txt"), "{msg}");
}
