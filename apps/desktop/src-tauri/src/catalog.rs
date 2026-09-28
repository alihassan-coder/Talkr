use std::path::Path;
use std::fs;
use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub version: u32,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogModel {
    pub id: String,
    pub kind: ModelKind,
    pub engine: String,
    pub name: String,
    pub description: String,
    pub languages: Vec<String>,
    pub size_bytes: u64,
    pub ram_recommended_bytes: u64,
    pub license: String,
    pub homepage: String,
    pub files: Vec<ModelFile>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Stt,
    Tts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub url: String,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub id: String,
    pub kind: ModelKind,
    pub name: String,
    pub engine: String,
    pub path: String,
    pub manifest: ModelManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub id: String,
    pub version: String,
    pub sha256: String,
    pub installed_at: i64,
    pub size_bytes: u64,
    pub files: Vec<String>,
}

impl Catalog {
    pub fn load_embedded() -> Result<Self> {
        let json = include_str!("../../catalog.json");
        let catalog: Catalog = serde_json::from_str(json)?;
        Ok(catalog)
    }

    pub fn get_model(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn models_by_kind(&self, kind: ModelKind) -> Vec<&CatalogModel> {
        self.models.iter().filter(|m| m.kind == kind).collect()
    }
}

impl CatalogModel {
    pub fn is_installed(&self, paths: &crate::paths::AppPaths) -> bool {
        let model_dir = paths.model_dir(
            match self.kind {
                ModelKind::Stt => "stt",
                ModelKind::Tts => "tts",
            },
            &self.id
        );
        model_dir.join("manifest.json").exists()
    }

    pub fn get_installed_manifest(&self, paths: &crate::paths::AppPaths) -> Option<ModelManifest> {
        let manifest_path = paths.model_dir(
            match self.kind {
                ModelKind::Stt => "stt",
                ModelKind::Tts => "tts",
            },
            &self.id
        ).join("manifest.json");

        if manifest_path.exists() {
            fs::read_to_string(manifest_path).ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        } else {
            None
        }
    }
}