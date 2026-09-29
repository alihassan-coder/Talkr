use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Path error: {0}")]
    Path(String),

    #[error("Model error: {0}")]
    Model(String),

    #[error("Download error: {0}")]
    Download(String),

    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

macro_rules! impl_from {
    ($variant:ident: $($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for AppError {
                fn from(err: $ty) -> Self {
                    AppError::$variant(err.to_string())
                }
            }
        )+
    };
}

impl_from!(Path: walkdir::Error);
impl_from!(
    Audio: symphonia::core::errors::Error,
    rubato::ResamplerConstructionError,
    rubato::ResampleError,
    hound::Error,
    cpal::DefaultStreamConfigError,
    cpal::BuildStreamError,
    cpal::PlayStreamError,
);
impl_from!(
    Other: tauri::Error,
    tauri_plugin_opener::Error,
    tokio::task::JoinError,
);

pub type Result<T> = std::result::Result<T, AppError>;
