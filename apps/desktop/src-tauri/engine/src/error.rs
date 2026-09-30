use talkr_protocol::FailureKind;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    /// Bad input: unreadable audio, empty text, a folder that is not a usable model.
    #[error("{0}")]
    Invalid(String),

    #[error("{0}")]
    OutOfMemory(String),

    #[error("{0}")]
    Engine(String),

    #[error("Cancelled")]
    Cancelled,
}

impl EngineError {
    pub fn kind(&self) -> FailureKind {
        match self {
            EngineError::Invalid(_) => FailureKind::Invalid,
            EngineError::OutOfMemory(_) => FailureKind::OutOfMemory,
            EngineError::Engine(_) => FailureKind::Engine,
            EngineError::Cancelled => FailureKind::Cancelled,
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::Invalid(e.to_string())
    }
}

macro_rules! audio_error {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for EngineError {
                fn from(e: $ty) -> Self {
                    EngineError::Invalid(format!("Audio error: {}", e))
                }
            }
        )+
    };
}

audio_error!(
    symphonia::core::errors::Error,
    rubato::ResamplerConstructionError,
    rubato::ResampleError,
    hound::Error,
);

pub type Result<T> = std::result::Result<T, EngineError>;
