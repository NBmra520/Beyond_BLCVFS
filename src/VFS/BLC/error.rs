use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlcError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid file format: {0}")]
    InvalidFormat(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    #[error("Chunk not found: {0}")]
    ChunkNotFound(String),
    
    #[error("Invalid offset: expected {expected}, got {actual}")]
    InvalidOffset { expected: usize, actual: usize },

    #[error("Index too large to allocate: {0} entries")]
    IndexAllocationFailed(usize),
}

pub type Result<T> = std::result::Result<T, BlcError>;
