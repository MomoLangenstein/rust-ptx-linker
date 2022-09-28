#[allow(clippy::module_name_repetitions)]
#[derive(Debug, thiserror::Error)]
pub enum LinkerError {
    #[error("No output path is specified")]
    NoOutputPathError,

    #[error("Expected path, got `{0}`")]
    PathArgumentError(String),

    #[error("Undefined references: {0:?}")]
    UndefinedReferences(Vec<String>),
}
