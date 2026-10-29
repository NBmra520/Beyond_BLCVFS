/// VFS container decryption for Arknights Endfield .chk files.
/// Handles header validation, parsing, descrambling, block decryption, and decompression.
/// Source: AnimeStudio — VFSFile.cs, Crypto/VFSUtils.cs
use super::crypto_constants::*;
use super::lz4_inv;
use super::vfs_aes;

// ── Helper readers ──

fn read_u8(buf: &[u8], offset: &mut usize) -> u8 {
    let v = buf[*offset];
    *offset += 1;
    v
}

fn read_u16_be(buf: &[u8], offset: &mut usize) -> u16 {
    let v = u16::from_be_bytes(buf[*offset..*offset + 2].try_into().unwrap());
    *offset += 2;
    v
}

fn read_u32_be(buf: &[u8], offset: &mut usize) -> u32 {
    let v = u32::from_be_bytes(buf[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    v
}

fn read_u32_le(buf: &[u8], offset: &mut usize) -> u32 {
    let v = u32::from_le_bytes(buf[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    v
}

// ── BitConcat helpers ──

/// Concatenate two u16 into a u32: (a << 16) | b
fn bit_concat_16(a: u16, b: u16) -> u32 {
    ((a as u32) << 16) | (b as u32)
}

/// Concatenate two u32 into a u64: (a << 32) | b
fn bit_concat_32(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | (b as u64)
}

/// Concatenate two u8 into a u16: (a << 8) | b
fn bit_concat_8(a: u8, b: u8) -> u16 {
    ((a as u16) << 8) | (b as u16)
}

// ── Header Validation ──

/// Check if the first 8 bytes form a valid VFS checksum pair.
/// Reads two big-endian u32 values and validates the checksum relationship.
pub fn is_valid_header(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let a = u32::from_be_bytes(data[0..4].try_into().unwrap());
    let b = u32::from_be_bytes(data[4..8].try_into().unwrap());
    let xored = a ^ HEADER_XOR_A;
    let c1 = 4u32.wrapping_mul(xored) & 0xFFFF0000;
    let c2 = xored.rotate_right(14);
    b == (c1 ^ c2 ^ HEADER_XOR_B)
}

// ── Header Structure ──

#[derive(Debug)]
pub struct VfsHeader {
    pub compressed_blocks_info_size: u32,
    pub uncompressed_blocks_info_size: u32,
    pub size: u64,
    pub flags: u32,
    pub enc_flags: u32,
}

impl VfsHeader {
    /// Compression method for block info: flags & 0x3F
    pub fn block_info_compression(&self) -> u32 {
        self.flags & 0x3F
    }

    /// Whether block info is at the end of the file
    pub fn block_info_at_end(&self) -> bool {
        (self.flags & 0x80) != 0
    }
}

/// Parse and descramble the VFS header from data (after skipping the 8-byte validation pair).
pub fn read_header(data: &[u8]) -> Result<VfsHeader, Box<dyn std::error::Error>> {
    let mut offset = 8; // skip the 8-byte validation pair

    // Read fields in AnimeStudio's specific order (reader is BigEndian)
    let cbs2 = read_u16_be(data, &mut offset);
    let flags2 = read_u32_be(data, &mut offset);
    let enc_flags = read_u32_be(data, &mut offset);
    let size2 = read_u32_be(data, &mut offset);
    let flags1 = read_u32_be(data, &mut offset);
    let ubs1 = read_u16_be(data, &mut offset);
    let _unknown = read_u32_be(data, &mut offset);
    let ubs2 = read_u16_be(data, &mut offset);
    let size1 = read_u32_be(data, &mut offset);
    let cbs1 = read_u16_be(data, &mut offset);
    let _unknown2 = read_u8(data, &mut offset);

    // Descramble
    let compressed_blocks_info_size =
        bit_concat_16(cbs1 ^ cbs2 ^ MERGE_XOR_16, cbs2).rotate_right(18) ^ CBI_UBI_FINAL_XOR;

    let uncompressed_blocks_info_size =
        bit_concat_16(ubs1 ^ ubs2 ^ MERGE_XOR_16, ubs2).rotate_right(18) ^ CBI_UBI_FINAL_XOR;

    let size =
        bit_concat_32(size1 ^ size2 ^ SIZE_MERGE_XOR, size2).rotate_right(18) ^ SIZE_FINAL_XOR;

    let enc_flags_descrambled = enc_flags ^ flags2;
    let flags = flags1 ^ flags2 ^ FLAGS_XOR;

    Ok(VfsHeader {
        compressed_blocks_info_size,
        uncompressed_blocks_info_size,
        size,
        flags,
        enc_flags: enc_flags_descrambled,
    })
}

// ── Block Info Structures ──

#[derive(Debug)]
pub struct VfsStorageBlock {
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub flags: u16,
}

impl VfsStorageBlock {
    /// Compression type: 0=none, 5=encrypted+LZ4Inv
    pub fn compression(&self) -> u16 {
        self.flags & 0x3F
    }
}

#[derive(Debug)]
pub struct VfsDirectoryNode {
    pub offset: u64,
    pub size: u64,
    pub flags: u32,
    pub name: String,
}

/// Parse the descrambled block info section.
pub fn parse_block_info(
    data: &[u8],
) -> Result<(Vec<VfsStorageBlock>, Vec<VfsDirectoryNode>), Box<dyn std::error::Error>> {
    let mut offset = 0;

    // Parse blocks count (C# temporarily switches reader to LittleEndian, XORs, then byte-swaps)
    let enc_count_raw = read_u32_le(data, &mut offset);
    let enc_count = (enc_count_raw ^ BLOCKS_COUNT_READ_XOR).swap_bytes();
    let low = (enc_count & 0xFFFF) as u16;
    let high = ((enc_count >> 16) & 0xFFFF) as u16;
    let blocks_count = bit_concat_16(low ^ high, low).rotate_right(18) ^ BLOCKS_COUNT_FINAL_XOR;

    let mut blocks = Vec::with_capacity(blocks_count as usize);
    for _ in 0..blocks_count {
        // Per-block fields read in BigEndian (reader restored to BE after count)
        let a = read_u16_be(data, &mut offset);
        let b = read_u16_be(data, &mut offset);
        let c = read_u16_be(data, &mut offset);
        let enc_flags = read_u16_be(data, &mut offset) ^ BLOCK_ENC_FLAGS_XOR;
        let d = read_u16_be(data, &mut offset);

        let a0 = (enc_flags & 0xFF) as u8;
        let a1 = ((enc_flags >> 8) & 0xFF) as u8;
        let flags = c ^ bit_concat_8(a0 ^ a1, a0).rotate_left(14) ^ BLOCK_FLAGS_XOR;

        let uncompressed_size =
            bit_concat_16(a ^ c ^ MERGE_XOR_16, c).rotate_right(18) ^ CBI_UBI_FINAL_XOR;
        let compressed_size =
            bit_concat_16(b ^ d ^ MERGE_XOR_16, d).rotate_right(18) ^ CBI_UBI_FINAL_XOR;

        blocks.push(VfsStorageBlock {
            compressed_size,
            uncompressed_size,
            flags,
        });
    }

    // Parse directory nodes count (C# temporarily switches to LE, XORs, then byte-swaps)
    let enc_nodes_raw = read_u32_le(data, &mut offset);
    let enc_nodes = (enc_nodes_raw ^ NODES_COUNT_READ_XOR).swap_bytes();
    let low = (enc_nodes & 0xFFFF) as u16;
    let high = ((enc_nodes >> 16) & 0xFFFF) as u16;
    let nodes_count = bit_concat_16(low ^ high, low).rotate_right(18) ^ NODES_COUNT_FINAL_XOR;

    let mut nodes = Vec::with_capacity(nodes_count as usize);
    for _ in 0..nodes_count {
        // Per-node fields read in BigEndian
        let a_raw = read_u32_be(data, &mut offset);
        let a = a_raw ^ NODE_A_XOR;
        let b = read_u32_be(data, &mut offset);
        let c = read_u32_be(data, &mut offset);
        let d = read_u32_be(data, &mut offset);

        // Read name: collect raw bytes until 0x00, then XOR for decryption
        let mut name_bytes = Vec::new();
        loop {
            if offset >= data.len() || name_bytes.len() >= 64 {
                break;
            }
            let byte = data[offset];
            offset += 1;
            if byte == 0 {
                break;
            }
            name_bytes.push(byte);
        }
        // Decrypt name bytes with per-position XOR
        for (j, b) in name_bytes.iter_mut().enumerate() {
            *b ^= (j as u8) ^ NODE_NAME_XOR;
        }
        let name = String::from_utf8_lossy(&name_bytes).to_string();

        let e = read_u32_be(data, &mut offset);

        let a0 = (a & 0xFFFF) as u16;
        let a1 = ((a >> 16) & 0xFFFF) as u16;
        let flags = bit_concat_16(a1 ^ a0, a0).rotate_right(18) ^ NODE_FLAGS_XOR ^ b;

        let node_offset = bit_concat_32(d ^ c ^ SIZE_MERGE_XOR, c).rotate_left(14) ^ SIZE_FINAL_XOR;
        let node_size = bit_concat_32(b ^ e ^ SIZE_MERGE_XOR, e).rotate_left(14) ^ SIZE_FINAL_XOR;

        nodes.push(VfsDirectoryNode {
            offset: node_offset,
            size: node_size,
            flags,
            name,
        });
    }

    Ok((blocks, nodes))
}

// ── Block Decryption (Scatter/Gather) ──

/// Decrypt a VFS data block using scatter/gather AES.
/// Small buffers (≤256 bytes) are decrypted as a whole.
/// Larger buffers use gather-decrypt-scatter on leading bytes of each 16-byte block.
pub fn decrypt_block(buffer: &mut [u8]) {
    if buffer.len() <= 256 {
        vfs_aes::decrypt(buffer, &VFS_AES_KEY, &VFS_AES_IV);
    } else {
        let num_blocks = buffer.len() / 16;
        let step = if num_blocks > 256 {
            1
        } else {
            256 / num_blocks
        };
        let count = num_blocks.min(256);

        // Gather
        let mut dec_buffer = vec![0u8; count * step];
        for i in 0..count {
            for s in 0..step {
                dec_buffer[i * step + s] = buffer[i * 16 + s];
            }
        }

        // Decrypt
        vfs_aes::decrypt(&mut dec_buffer, &VFS_AES_KEY, &VFS_AES_IV);

        // Scatter
        for i in 0..count {
            for s in 0..step {
                buffer[i * 16 + s] = dec_buffer[i * step + s];
            }
        }
    }
}

// ── Full VFS File Processing ──

/// A processed VFS file with its extracted entries.
#[derive(Debug)]
pub struct VfsFile {
    pub header: VfsHeader,
    pub blocks: Vec<VfsStorageBlock>,
    pub nodes: Vec<VfsDirectoryNode>,
}

/// Process a VFS-encrypted .chk file and return the decrypted+decompressed data stream.
///
/// Steps:
/// 1. Validate header checksum (8 bytes)
/// 2. Read and descramble header
/// 3. Locate block info (at end of file, or after header)
/// 4. Decrypt block info
/// 5. Decompress block info (standard LZ4)
/// 6. Parse blocks and directory entries
/// 7. For each storage block: decrypt + decompress as needed
/// 8. Return assembled output with file entries
pub fn process_vfs(data: &[u8]) -> Result<(VfsFile, Vec<u8>), Box<dyn std::error::Error>> {
    // 1. Validate
    if !is_valid_header(data) {
        return Err("Invalid VFS header checksum".into());
    }

    // 2. Read header
    let header = read_header(data)?;

    // 3. Locate block info
    let header_end = if header.enc_flags >= 7 { 48 } else { 40 };

    let blocks_info_offset = if header.block_info_at_end() {
        header.size as usize - header.compressed_blocks_info_size as usize
    } else {
        header_end
    };

    // 4. Read block info (decrypt + decompress only when compressed)
    let bi_end = blocks_info_offset + header.compressed_blocks_info_size as usize;
    if bi_end > data.len() {
        return Err(format!(
            "Block info extends beyond file: offset={}, size={}, file_len={}",
            blocks_info_offset,
            header.compressed_blocks_info_size,
            data.len()
        )
        .into());
    }

    let block_info_decompressed = if header.block_info_compression() != 0 {
        // Compressed + encrypted: decrypt then LZ4 decompress
        let mut block_info_compressed = data[blocks_info_offset..bi_end].to_vec();
        decrypt_block(&mut block_info_compressed);
        lz4_flex::decompress(
            &block_info_compressed,
            header.uncompressed_blocks_info_size as usize,
        )
        .map_err(|e| format!("LZ4 decompress block info: {}", e))?
    } else {
        // Uncompressed: use raw bytes directly
        data[blocks_info_offset..bi_end].to_vec()
    };

    // 6. Parse blocks and directory entries
    let (blocks, nodes) = parse_block_info(&block_info_decompressed)?;

    // 7. Decrypt and decompress each storage block
    let data_offset = if header.block_info_at_end() {
        header_end
    } else {
        let mut bi_size = header.compressed_blocks_info_size as usize;
        // BlockInfoNeedPaddingAtStart (flag 0x200): align to 16 bytes
        if (header.flags & 0x200) != 0 {
            bi_size = (bi_size + 15) & !15;
        }
        header_end + bi_size
    };

    let mut output = Vec::new();
    let mut current_data_offset = data_offset;

    for block in &blocks {
        let block_end = current_data_offset + block.compressed_size as usize;
        if block_end > data.len() {
            return Err(format!(
                "Storage block extends beyond file: offset={}, size={}, file_len={}",
                current_data_offset,
                block.compressed_size,
                data.len()
            )
            .into());
        }

        let mut block_data = data[current_data_offset..block_end].to_vec();

        match block.compression() {
            0 => {
                // Uncompressed — just copy
                output.extend_from_slice(&block_data);
            }
            5 => {
                // Encrypted + LZ4Inv compressed
                decrypt_block(&mut block_data);
                let decompressed =
                    lz4_inv::decompress(&block_data, block.uncompressed_size as usize)?;
                output.extend_from_slice(&decompressed);
            }
            other => {
                return Err(format!("Unknown VFS block compression type: {}", other).into());
            }
        }

        current_data_offset = block_end;
    }

    let vfs = VfsFile {
        header,
        blocks,
        nodes,
    };

    Ok((vfs, output))
}

/// Extract individual files from the decompressed VFS data stream using directory nodes.
pub fn extract_files(vfs: &VfsFile, data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for node in &vfs.nodes {
        let start = node.offset as usize;
        let end = start + node.size as usize;
        if end <= data.len() {
            files.push((node.name.clone(), data[start..end].to_vec()));
        }
    }
    files
}

/// Process a single VFS container at a given offset within `data`.
/// Returns (VfsFile, decompressed_output, bytes_consumed) so the caller can advance past it.
pub fn process_vfs_at(
    data: &[u8],
    offset: usize,
) -> Result<(VfsFile, Vec<u8>, usize), Box<dyn std::error::Error>> {
    let container = &data[offset..];
    let (vfs, output) = process_vfs(container)?;

    let header_end = if vfs.header.enc_flags >= 7 { 48 } else { 40 };
    let mut bi_size = vfs.header.compressed_blocks_info_size as usize;
    if (vfs.header.flags & 0x200) != 0 {
        bi_size = (bi_size + 15) & !15;
    }
    let data_start = if vfs.header.block_info_at_end() {
        header_end
    } else {
        header_end + bi_size
    };
    let total_block_data: usize = vfs.blocks.iter().map(|b| b.compressed_size as usize).sum();
    let consumed = data_start + total_block_data;

    Ok((vfs, output, consumed))
}

/// A single processed VFS container result.
pub struct VfsContainerResult {
    pub offset: usize,
    pub vfs: VfsFile,
    pub data: Vec<u8>,
}

/// Process all VFS containers in a .chk file.
/// Scans for valid VFS headers and processes each container sequentially.
pub fn process_all_vfs(data: &[u8]) -> Result<Vec<VfsContainerResult>, Box<dyn std::error::Error>> {
    let mut results = Vec::new();
    let mut pos = 0;

    while pos + 8 <= data.len() {
        if !is_valid_header(&data[pos..]) {
            pos += 1;
            continue;
        }

        match process_vfs_at(data, pos) {
            Ok((vfs, output, consumed)) => {
                results.push(VfsContainerResult {
                    offset: pos,
                    vfs,
                    data: output,
                });
                pos += consumed;
            }
            Err(_) => {
                pos += 8;
            }
        }
    }

    Ok(results)
}
