/// Custom AES-128 implementation for VFS container encryption.
/// Uses a non-standard S-box and CTR-like keystream mode with custom IV derivation.
/// Source: AnimeStudio — Crypto/VFSUtils.cs (VFSAES class)
use super::crypto_constants::{RCON, VFS_AES_SBOX, VFS_XOR_KEY};

/// Expand the 16-byte key into 11 round keys (AES-128 = 10 rounds + initial).
fn key_expansion(key: &[u8; 16], sbox: &[u8; 256]) -> [[u8; 16]; 11] {
    let mut round_keys = [[0u8; 16]; 11];
    round_keys[0] = *key;

    for i in 1..11 {
        let prev = &round_keys[i - 1];
        let mut next = [0u8; 16];

        // RotWord + SubWord + Rcon on last 4 bytes of previous key
        let rot = [
            sbox[prev[13] as usize] ^ RCON[i],
            sbox[prev[14] as usize],
            sbox[prev[15] as usize],
            sbox[prev[12] as usize],
        ];

        // First 4 bytes
        for j in 0..4 {
            next[j] = prev[j] ^ rot[j];
        }

        // Remaining 12 bytes
        for j in 4..16 {
            next[j] = prev[j] ^ next[j - 4];
        }

        round_keys[i] = next;
    }

    round_keys
}

/// SubBytes: substitute each byte using the custom S-box.
fn sub_bytes(state: &mut [u8; 16], sbox: &[u8; 256]) {
    for b in state.iter_mut() {
        *b = sbox[*b as usize];
    }
}

/// ShiftRows: standard AES shift rows on a 4x4 column-major state.
fn shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift left 1
    let tmp = state[1];
    state[1] = state[5];
    state[5] = state[9];
    state[9] = state[13];
    state[13] = tmp;

    // Row 2: shift left 2
    let (tmp0, tmp1) = (state[2], state[6]);
    state[2] = state[10];
    state[6] = state[14];
    state[10] = tmp0;
    state[14] = tmp1;

    // Row 3: shift left 3 (= shift right 1)
    let tmp = state[15];
    state[15] = state[11];
    state[11] = state[7];
    state[7] = state[3];
    state[3] = tmp;
}

/// Galois field multiplication by 2 in GF(2^8) with AES polynomial.
fn gmul2(a: u8) -> u8 {
    let mut result = (a as u16) << 1;
    if a & 0x80 != 0 {
        result ^= 0x1B;
    }
    result as u8
}

/// MixColumns: standard AES mix columns.
fn mix_columns(state: &mut [u8; 16]) {
    for col in 0..4 {
        let i = col * 4;
        let (a0, a1, a2, a3) = (state[i], state[i + 1], state[i + 2], state[i + 3]);

        state[i] = gmul2(a0) ^ gmul2(a1) ^ a1 ^ a2 ^ a3;
        state[i + 1] = a0 ^ gmul2(a1) ^ gmul2(a2) ^ a2 ^ a3;
        state[i + 2] = a0 ^ a1 ^ gmul2(a2) ^ gmul2(a3) ^ a3;
        state[i + 3] = gmul2(a0) ^ a0 ^ a1 ^ a2 ^ gmul2(a3);
    }
}

/// AddRoundKey: XOR state with round key.
fn add_round_key(state: &mut [u8; 16], round_key: &[u8; 16]) {
    for i in 0..16 {
        state[i] ^= round_key[i];
    }
}

/// AES-128 encrypt a single 16-byte block using the custom S-box.
fn aes_encrypt_block(block: &[u8; 16], round_keys: &[[u8; 16]; 11]) -> [u8; 16] {
    let mut state = *block;

    // Initial round key addition
    add_round_key(&mut state, &round_keys[0]);

    // Rounds 1-9: SubBytes, ShiftRows, MixColumns, AddRoundKey
    for round_key in round_keys.iter().take(10).skip(1) {
        sub_bytes(&mut state, &VFS_AES_SBOX);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, round_key);
    }

    // Final round (no MixColumns)
    sub_bytes(&mut state, &VFS_AES_SBOX);
    shift_rows(&mut state);
    add_round_key(&mut state, &round_keys[10]);

    state
}

/// Derive the next IV from the current keystream block.
/// Each byte: temp = keystream[i] ^ (31*i) ^ (XORKey >> (count & 0x38))
///            temp = ROR8(temp, 5)
///            temp = SBOX[temp]
fn derive_next_iv(keystream: &[u8; 16], xor_key: u64) -> [u8; 16] {
    let mut next_iv = [0u8; 16];
    for i in 0..16 {
        let count = (i as u32) * 8;
        let shift_src = (xor_key >> (count & 0x38)) as u8;
        let temp = keystream[i] ^ ((31u16 * i as u16) as u8) ^ shift_src;
        // Rotate right 5 bits = (temp >> 5) | (temp << 3)
        let rotated = temp.rotate_left(3);
        next_iv[i] = VFS_AES_SBOX[rotated as usize];
    }
    next_iv
}

/// Decrypt a buffer using the custom AES-CTR-like mode.
///
/// For each 16-byte block:
/// 1. keystream = AES_ENCRYPT(current_iv, key)
/// 2. plaintext = ciphertext ^ keystream
/// 3. Derive next IV from keystream
pub fn decrypt(data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    let round_keys = key_expansion(key, &VFS_AES_SBOX);
    let mut current_iv = *iv;

    let full_blocks = data.len() / 16;
    for block_idx in 0..full_blocks {
        let offset = block_idx * 16;

        // Generate keystream by encrypting current IV
        let keystream = aes_encrypt_block(&current_iv, &round_keys);

        // XOR ciphertext with keystream to get plaintext
        for i in 0..16 {
            data[offset + i] ^= keystream[i];
        }

        // Derive next IV
        current_iv = derive_next_iv(&keystream, VFS_XOR_KEY);
    }

    // Handle remaining bytes (partial last block)
    let remaining = data.len() % 16;
    if remaining > 0 {
        let offset = full_blocks * 16;
        let keystream = aes_encrypt_block(&current_iv, &round_keys);
        for i in 0..remaining {
            data[offset + i] ^= keystream[i];
        }
    }
}
