use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::error::Result;
use crate::paths::AppPaths;

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
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Stt,
    Tts,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelKind::Stt => "stt",
            ModelKind::Tts => "tts",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stt" => Some(ModelKind::Stt),
            "tts" => Some(ModelKind::Tts),
            _ => None,
        }
    }
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

impl ModelManifest {
    pub fn read(dir: &std::path::Path) -> Option<Self> {
        fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }
}

impl Catalog {
    pub fn load_embedded() -> Result<Self> {
        let json = include_str!("../../../../packages/model-catalog/catalog.json");
        let catalog: Catalog = serde_json::from_str(json)?;
        Ok(catalog)
    }

    pub fn get_model(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }
}

impl CatalogModel {
    pub fn dir(&self, paths: &AppPaths) -> PathBuf {
        paths.model_dir(self.kind.as_str(), &self.id)
    }

    pub fn is_installed(&self, paths: &AppPaths) -> bool {
        self.dir(paths).join("manifest.json").exists()
    }

    pub fn get_installed_manifest(&self, paths: &AppPaths) -> Option<ModelManifest> {
        ModelManifest::read(&self.dir(paths))
    }
}
