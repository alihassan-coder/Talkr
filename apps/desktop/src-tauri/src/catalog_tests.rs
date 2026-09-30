//! Tests for `catalog.rs`: model id validation and invariants of the embedded catalog.

use super::*;
use std::collections::HashSet;

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[test]
fn accepts_well_formed_ids() {
    for id in [
        "whisper-tiny-en",
        "kokoro-multi-lang-v1_0",
        "piper-en_US-lessac-medium",
        "ggml-base.en",
        "a",
        "A.b_c-1",
        "..a",
        "a..b",
        ".hidden",
        &"x".repeat(MAX_MODEL_ID_LEN),
    ] {
        assert!(is_valid_model_id(id), "{id:?} should be valid");
        assert!(validate_model_id(id).is_ok());
    }
}

#[test]
fn rejects_unsafe_ids() {
    for id in [
        "",
        ".",
        "..",
        "../evil",
        "..\\evil",
        "a/b",
        "a\\b",
        "C:",
        "C:evil",
        "C:\\Windows",
        "\\\\server\\share",
        "/etc/passwd",
        "model:stream",
        "has space",
        "tab\there",
        "nul\0byte",
        "caf\u{e9}",
        "emoji\u{1F600}",
        "star*",
        "q?",
        "pipe|",
        "<lt>",
        "quote\"",
        &"x".repeat(MAX_MODEL_ID_LEN + 1),
    ] {
        assert!(!is_valid_model_id(id), "{id:?} should be rejected");
        assert!(matches!(validate_model_id(id), Err(AppError::Validation(_))));
    }
}

#[test]
fn embedded_catalog_parses_and_is_not_empty() {
    let catalog = Catalog::load_embedded().expect("embedded catalog must parse");
    assert_eq!(catalog.version, 1);
    assert!(catalog.models.iter().any(|m| m.kind == ModelKind::Stt));
    assert!(catalog.models.iter().any(|m| m.kind == ModelKind::Tts));
}

#[test]
fn catalog_ids_are_unique_and_valid() {
    let catalog = Catalog::load_embedded().unwrap();
    let mut seen = HashSet::new();
    for m in &catalog.models {
        assert!(is_valid_model_id(&m.id), "invalid id {:?}", m.id);
        assert!(seen.insert(m.id.as_str()), "duplicate id {:?}", m.id);
        assert!(catalog.get_model(&m.id).is_some());
    }
}

#[test]
fn catalog_files_have_checksums_and_https_urls() {
    let catalog = Catalog::load_embedded().unwrap();
    for m in &catalog.models {
        assert!(!m.files.is_empty(), "{} has no files", m.id);
        for f in &m.files {
            assert!(is_hex(&f.sha256, 64), "{}: bad sha256 {:?}", m.id, f.sha256);
            assert_eq!(f.sha256, f.sha256.to_lowercase(), "{}: sha256 must be lowercase", m.id);
            let url = reqwest::Url::parse(&f.url).unwrap_or_else(|e| panic!("{}: bad url: {e}", m.id));
            assert_eq!(url.scheme(), "https", "{}: url must be https", m.id);
            assert!(url.host_str().is_some_and(|h| !h.is_empty()));
        }
    }
}

#[test]
fn hugging_face_urls_are_pinned_to_a_commit() {
    let catalog = Catalog::load_embedded().unwrap();
    let mut checked = 0;
    for f in catalog.models.iter().flat_map(|m| &m.files) {
        let url = reqwest::Url::parse(&f.url).unwrap();
        if url.host_str() != Some("huggingface.co") {
            continue;
        }
        // https://huggingface.co/<org>/<repo>/resolve/<revision>/<file>
        let segments: Vec<&str> = url.path_segments().unwrap().collect();
        assert!(segments.len() >= 5, "unexpected HF url {}", f.url);
        assert_eq!(segments[2], "resolve", "HF url must use /resolve/: {}", f.url);
        assert!(is_hex(segments[3], 40), "HF url not pinned to a 40-hex commit: {}", f.url);
        checked += 1;
    }
    assert!(checked > 0, "expected at least one Hugging Face url");
}

#[test]
fn catalog_sizes_are_positive() {
    let catalog = Catalog::load_embedded().unwrap();
    for m in &catalog.models {
        assert!(m.size_bytes > 0, "{}: size_bytes must be > 0", m.id);
        assert!(m.ram_recommended_bytes > 0, "{}: ram_recommended_bytes must be > 0", m.id);
    }
}

#[test]
fn stt_models_are_plain_files_and_tts_models_are_tar_bz2() {
    let catalog = Catalog::load_embedded().unwrap();
    for m in &catalog.models {
        for f in &m.files {
            match m.kind {
                ModelKind::Stt => {
                    assert!(f.archive.is_none(), "{}: STT model must not be an archive", m.id);
                    assert_eq!(m.engine, "whisper", "{}", m.id);
                }
                ModelKind::Tts => {
                    assert_eq!(f.archive.as_deref(), Some("tar.bz2"), "{}: TTS model must be tar.bz2", m.id);
                    assert_eq!(m.engine, "sherpa-onnx", "{}", m.id);
                }
            }
        }
    }
}

#[test]
fn model_kind_round_trips() {
    for k in [ModelKind::Stt, ModelKind::Tts] {
        assert_eq!(ModelKind::parse(k.as_str()), Some(k));
    }
    assert_eq!(ModelKind::parse("STT"), None);
    assert_eq!(ModelKind::parse(""), None);
}
