//! Error types for Three.js compiler and importer operations.

use thiserror::Error;

/// Errors produced during Three.js project compilation or scene import/export.
#[derive(Error, Debug)]
pub enum ThreeJsError {
    #[error("JSON serialization or deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Foreplay Scene IR error: {0}")]
    Ir(#[from] foreplay_ir::IrError),

    #[error("Invalid Three.js scene structure: {0}")]
    InvalidScene(String),

    #[error("Project compilation error: {0}")]
    CompilationError(String),

    #[error("Matrix decomposition error: {0}")]
    MatrixDecomposition(String),

    #[error("Script transpilation error: {0}")]
    Transpilation(String),
}
