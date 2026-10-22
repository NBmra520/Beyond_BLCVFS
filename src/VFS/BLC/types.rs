use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UInt128(pub [u8; 16]);

impl UInt128 {
    pub fn from_bytes(data: [u8; 16]) -> Self {
        Self(data)
    }
    
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl fmt::Display for UInt128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub file_name: String,
    pub file_name_hash: u64,
    pub file_chunk_md5_name: UInt128,
    pub file_data_md5: UInt128,
    pub offset: i64,
    pub len: i64,
    pub block_type: u8,
    pub b_use_encrypt: bool,
    pub iv_seed: Option<i64>,
    pub file_tag: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub md5_name: UInt128,
    pub content_md5: UInt128,
    pub length: i64,
    pub block_type: u8,
    pub file_tag: i64,
    pub files_count: i32,
    pub files: Vec<FileInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlcMainInfo {
    pub code_version: i32,
    pub version: i32,
    pub group_cfg_name: String,
    pub group_cfg_hash_name: u64,
    pub group_file_info_num: i32,
    pub group_chunks_length: i64,
    pub block_type: u8,
    pub all_chunks_count: i32,
    pub all_chunks: Vec<ChunkInfo>,
}
