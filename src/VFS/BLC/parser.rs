use super::types::*;
use super::error::{BlcError, Result};
use std::io::{Read, Cursor};
use byteorder::{LittleEndian, ReadBytesExt};

pub struct BlcParser;

impl BlcParser {
    pub fn parse(data: &[u8]) -> Result<BlcMainInfo> {
        let mut cursor = Cursor::new(data);
        Self::parse_main_info(&mut cursor)
    }

    fn parse_main_info(cursor: &mut Cursor<&[u8]>) -> Result<BlcMainInfo> {
        let raw_version = cursor.read_i32::<LittleEndian>()?;
        let (code_version, version) = if raw_version < 11 {
            (raw_version, cursor.read_i32::<LittleEndian>()?)
        } else {
            (3, raw_version)
        };

        let name_len = cursor.read_u16::<LittleEndian>()? as usize;
        let group_cfg_name = if name_len > 0 {
            let mut name_buf = vec![0u8; name_len];
            cursor.read_exact(&mut name_buf)?;
            String::from_utf8_lossy(&name_buf).to_string()
        } else {
            String::new()
        };

        let group_cfg_hash_name = cursor.read_u64::<LittleEndian>()?;

        let group_file_info_num = cursor.read_i32::<LittleEndian>()?;
        let group_chunks_length = cursor.read_i64::<LittleEndian>()?;
        let block_type = cursor.read_u8()?;
        let all_chunks_count = cursor.read_i32::<LittleEndian>()?;

        let mut all_chunks = Vec::new();
        all_chunks
            .try_reserve(all_chunks_count as usize)
            .map_err(|_| BlcError::IndexAllocationFailed(all_chunks_count as usize))?;
        for _ in 0..all_chunks_count {
            all_chunks.push(Self::parse_chunk_info(cursor, code_version)?);
        }

        if code_version >= 5 {
            let mut trailer = [0u8; 10];
            cursor.read_exact(&mut trailer).ok();
        }

        Ok(BlcMainInfo {
            code_version,
            version,
            group_cfg_name,
            group_cfg_hash_name,
            group_file_info_num,
            group_chunks_length,
            block_type,
            all_chunks_count,
            all_chunks,
        })
    }

    fn read_file_tag(cursor: &mut Cursor<&[u8]>, code_version: i32) -> Result<i64> {
        if code_version <= 3 {
            return Ok(0);
        }
        if code_version >= 5 {
            return Ok(cursor.read_i64::<LittleEndian>()?);
        }
        Ok(cursor.read_i32::<LittleEndian>()? as i64)
    }

    fn parse_chunk_info(cursor: &mut Cursor<&[u8]>, code_version: i32) -> Result<ChunkInfo> {
        let mut md5_name_bytes = [0u8; 16];
        cursor.read_exact(&mut md5_name_bytes)?;
        let md5_name = UInt128::from_bytes(md5_name_bytes);

        let mut content_md5_bytes = [0u8; 16];
        cursor.read_exact(&mut content_md5_bytes)?;
        let content_md5 = UInt128::from_bytes(content_md5_bytes);

        let length = cursor.read_i64::<LittleEndian>()?;
        let block_type = cursor.read_u8()?;
        let file_tag = Self::read_file_tag(cursor, code_version)?;
        let files_count = cursor.read_i32::<LittleEndian>()?;

        let mut files = Vec::new();
        files
            .try_reserve(files_count as usize)
            .map_err(|_| BlcError::IndexAllocationFailed(files_count as usize))?;
        for _ in 0..files_count {
            files.push(Self::parse_file_info(cursor, code_version)?);
        }

        Ok(ChunkInfo {
            md5_name,
            content_md5,
            length,
            block_type,
            file_tag,
            files_count,
            files,
        })
    }

    fn parse_file_info(cursor: &mut Cursor<&[u8]>, code_version: i32) -> Result<FileInfo> {
        let name_len = cursor.read_u16::<LittleEndian>()? as usize;
        let file_name = if name_len > 0 {
            let mut name_buf = vec![0u8; name_len];
            cursor.read_exact(&mut name_buf)?;
            String::from_utf8_lossy(&name_buf).to_string()
        } else {
            String::new()
        };

        let file_name_hash = cursor.read_u64::<LittleEndian>()?;

        let mut file_chunk_md5_name_bytes = [0u8; 16];
        cursor.read_exact(&mut file_chunk_md5_name_bytes)?;
        let file_chunk_md5_name = UInt128::from_bytes(file_chunk_md5_name_bytes);

        let mut file_data_md5_bytes = [0u8; 16];
        cursor.read_exact(&mut file_data_md5_bytes)?;
        let file_data_md5 = UInt128::from_bytes(file_data_md5_bytes);

        let offset = cursor.read_i64::<LittleEndian>()?;
        let len = cursor.read_i64::<LittleEndian>()?;
        let block_type = cursor.read_u8()?;
        let b_use_encrypt = cursor.read_u8()? != 0;

        let iv_seed = if b_use_encrypt {
            Some(cursor.read_i64::<LittleEndian>()?)
        } else {
            None
        };

        let file_tag = Self::read_file_tag(cursor, code_version)?;

        Ok(FileInfo {
            file_name,
            file_name_hash,
            file_chunk_md5_name,
            file_data_md5,
            offset,
            len,
            block_type,
            b_use_encrypt,
            iv_seed,
            file_tag,
        })
    }
}
