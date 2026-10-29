//! LZ4Inv decompressor — a modified LZ4 variant used by Arknights Endfield VFS.
//! Differences from standard LZ4:
//! - Token nibbles are bit-interleaved (masks 0b00110011 / 0b11001100)
//! - Back-reference offset is big-endian u16
//!
//! Source: AnimeStudio — LZ4/LZ4Inv.cs

/// Parse the bit-interleaved token byte into (literal_count, match_count).
pub(crate) fn parse_token(val: u8) -> (u8, u8) {
    let lit = val & 0b0011_0011; // bits 0,1,4,5
    let enc = (val & 0b1100_1100) >> 2; // bits 2,3,6,7 shifted down by 2

    // Reassemble each as a 4-bit value:
    // low 2 bits stay, bits 4-5 become bits 2-3
    let lit_count = (lit & 0b11) | ((lit >> 2) & 0b1100);
    let enc_count = (enc & 0b11) | ((enc >> 2) & 0b1100);

    (lit_count, enc_count)
}

/// Read a big-endian u16 back-reference offset.
pub(crate) fn read_back_offset(data: &[u8], pos: usize) -> u16 {
    ((data[pos] as u16) << 8) | (data[pos + 1] as u16)
}

/// Read additional length bytes (when nibble == 15).
/// Returns (additional_length, bytes_consumed).
fn read_extra_length(data: &[u8], mut pos: usize) -> (usize, usize) {
    let mut extra: usize = 0;
    loop {
        let b = data[pos] as usize;
        pos += 1;
        extra += b;
        if b != 255 {
            break;
        }
    }
    (extra, pos)
}

/// Decompress LZ4Inv compressed data.
///
/// # Arguments
/// * `compressed` - The compressed data bytes
/// * `uncompressed_size` - Expected output size
///
/// # Returns
/// Decompressed bytes, or an error if decompression fails.
pub fn decompress(
    compressed: &[u8],
    uncompressed_size: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut output = vec![0u8; uncompressed_size];
    let mut cmp_pos: usize = 0;
    let mut dec_pos: usize = 0;

    while cmp_pos < compressed.len() && dec_pos < uncompressed_size {
        // 1. Read token
        let token = compressed[cmp_pos];
        cmp_pos += 1;
        let (lit_count, match_count) = parse_token(token);

        // 2. Extended literal length
        let mut literal_len = lit_count as usize;
        if lit_count == 15 {
            let (extra, new_pos) = read_extra_length(compressed, cmp_pos);
            cmp_pos = new_pos;
            literal_len += extra;
        }

        // 3. Copy literals
        if literal_len > 0 {
            if cmp_pos + literal_len > compressed.len() {
                return Err(format!(
                    "LZ4Inv: literal overrun at cmp_pos={}, lit_len={}, cmp_len={}",
                    cmp_pos,
                    literal_len,
                    compressed.len()
                )
                .into());
            }
            if dec_pos + literal_len > uncompressed_size {
                return Err(format!(
                    "LZ4Inv: output overrun at dec_pos={}, lit_len={}, out_size={}",
                    dec_pos, literal_len, uncompressed_size
                )
                .into());
            }
            output[dec_pos..dec_pos + literal_len]
                .copy_from_slice(&compressed[cmp_pos..cmp_pos + literal_len]);
            cmp_pos += literal_len;
            dec_pos += literal_len;
        }

        // If we've filled the output, we're done
        if dec_pos >= uncompressed_size {
            break;
        }

        // 4. Read back-reference offset (big-endian u16)
        if cmp_pos + 2 > compressed.len() {
            return Err(format!(
                "LZ4Inv: unexpected end reading back offset at cmp_pos={}",
                cmp_pos
            )
            .into());
        }
        let back_offset = read_back_offset(compressed, cmp_pos) as usize;
        cmp_pos += 2;

        if back_offset > dec_pos {
            return Err(format!(
                "LZ4Inv: invalid back offset {} at dec_pos={}",
                back_offset, dec_pos
            )
            .into());
        }

        // 5. Extended match length (base = 4)
        let mut match_len = (match_count as usize) + 4;
        if match_count == 15 {
            let (extra, new_pos) = read_extra_length(compressed, cmp_pos);
            cmp_pos = new_pos;
            match_len += extra;
        }

        // 6. Copy match (sequential, byte-by-byte to handle overlapping)
        // back_offset=0 is valid: self-copy from current position (keeps existing values)
        for enc_pos in (dec_pos - back_offset..).take(match_len) {
            if dec_pos >= uncompressed_size {
                break;
            }
            output[dec_pos] = output[enc_pos];
            dec_pos += 1;
        }
    }

    if dec_pos != uncompressed_size {
        return Err(format!(
            "LZ4Inv: size mismatch: got {} bytes, expected {}",
            dec_pos, uncompressed_size
        )
        .into());
    }

    Ok(output)
}
