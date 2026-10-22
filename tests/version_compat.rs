use std::fs;

use blc_vfs::MultiVFS;
use blc_vfs::VFS::BLC::BlcParser;
use chacha20::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};
use chacha20::ChaCha20;

const CHACHA_KEY: [u8; 32] = [
    0xe9, 0x5b, 0x31, 0x7a, 0xc4, 0xf8, 0x28, 0x56, 0x9d, 0x23, 0xa8, 0x6b, 0xf2, 0x71, 0xdc, 0xb5,
    0x3e, 0x84, 0x6f, 0xa7, 0x5c, 0x92, 0x4d, 0x67, 0x1d, 0xba, 0x8e, 0x38, 0xf4, 0xca, 0x52, 0xe1,
];
const LEGACY_VERSION_CONST: i32 = 0x0198_A899;
const V5_FILE_TAG: i64 = 0x0020_9C85_5B11_B951;
const VFS_PROTO_VERSION: i32 = 3;
const CHUNK_MD5: u128 = 0xDEAD_BEEF_DEAD_BEEF_DEAD_BEEF_DEAD_BEEF;
const B_IV_SEED: i64 = 0x1122_3344_5566_7788;

struct W {
    buf: Vec<u8>,
}

impl W {
    fn new() -> Self {
        W { buf: Vec::new() }
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    fn u128(&mut self, v: u128) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u16(s.len() as u16);
        self.bytes(s.as_bytes());
    }
}

fn push_tag(w: &mut W, code_version: i32, value_i32: i32, value_i64: i64) {
    match code_version {
        0..=3 => {}
        4 => w.i32(value_i32),
        _ => w.i64(value_i64),
    }
}

fn build_payload(code_version: i32, version: i32, trailer: bool) -> Vec<u8> {
    let mut w = W::new();
    if code_version <= 3 {
        w.i32(version);
    } else {
        w.i32(code_version);
        w.i32(version);
    }

    w.str("Bundle");
    w.i64(0x0102_0304_0506_0708);
    w.i32(2);
    w.i64(0x1234);
    w.u8(1);
    w.i32(1);

    w.u128(CHUNK_MD5);
    w.u128(0x1111_1111_1111_1111_1111_1111_1111_1111);
    w.i64(0x1000);
    w.u8(1);
    push_tag(&mut w, code_version, 1, 1);
    w.i32(2);

    w.str("a.bin");
    w.i64(0xA1A1_A1A1_A1A1_A1A1u64 as i64);
    w.u128(CHUNK_MD5);
    w.u128(0x3333_3333_3333_3333_3333_3333_3333_3333);
    w.i64(0);
    w.i64(128);
    w.u8(1);
    w.u8(0);
    push_tag(&mut w, code_version, 1, V5_FILE_TAG);

    w.str("b.bin");
    w.i64(0xB2B2_B2B2_B2B2_B2B2u64 as i64);
    w.u128(CHUNK_MD5);
    w.u128(0x5555_5555_5555_5555_5555_5555_5555_5555);
    w.i64(256);
    w.i64(512);
    w.u8(1);
    w.u8(1);
    w.i64(B_IV_SEED);
    push_tag(&mut w, code_version, 1, 2);

    if trailer {
        w.bytes(&[0x2A; 10]);
    }
    w.buf
}

fn check_info(info: &blc_vfs::VFS::BLC::BlcMainInfo, code_version: i32, version: i32) {
    assert_eq!(info.code_version, code_version, "codeVersion 判定错误");
    assert_eq!(info.version, version);
    assert_eq!(info.group_cfg_name, "Bundle");
    assert_eq!(info.group_file_info_num, 2);
    assert_eq!(info.all_chunks.len(), 1);

    let chunk = &info.all_chunks[0];
    assert_eq!(chunk.files.len(), 2, "filesCount 解析错位（tag 宽度错误）");

    let f0 = &chunk.files[0];
    assert_eq!(f0.file_name, "a.bin");
    assert!(!f0.b_use_encrypt);
    assert_eq!(f0.len, 128);

    let f1 = &chunk.files[1];
    assert_eq!(f1.file_name, "b.bin");
    assert!(f1.b_use_encrypt);
    assert_eq!(f1.iv_seed, Some(B_IV_SEED));
    assert_eq!(f1.len, 512);

    match code_version {
        5 => {
            assert_eq!(chunk.file_tag, 1);
            assert_eq!(f0.file_tag, V5_FILE_TAG);
            assert_eq!(f1.file_tag, 2);
        }
        4 => {
            assert_eq!(chunk.file_tag, 1);
            assert_eq!(f0.file_tag, 1);
            assert_eq!(f1.file_tag, 1);
        }
        _ => {
            assert_eq!(chunk.file_tag, 0);
            assert_eq!(f0.file_tag, 0);
            assert_eq!(f1.file_tag, 0);
        }
    }
}

#[test]
fn parse_v5_new_format() {
    let data = build_payload(5, LEGACY_VERSION_CONST, true);
    check_info(&BlcParser::parse(&data).unwrap(), 5, LEGACY_VERSION_CONST);
}

#[test]
fn parse_v4_i32_tag() {
    let data = build_payload(4, LEGACY_VERSION_CONST, false);
    check_info(&BlcParser::parse(&data).unwrap(), 4, LEGACY_VERSION_CONST);
}

#[test]
fn parse_v3_legacy_format() {
    let data = build_payload(3, LEGACY_VERSION_CONST, false);
    check_info(&BlcParser::parse(&data).unwrap(), 3, LEGACY_VERSION_CONST);
}

fn keystream_xor(nonce: &[u8; 12], data: &[u8]) -> Vec<u8> {
    let mut cipher = ChaCha20::new((&CHACHA_KEY).into(), nonce.into());
    cipher.seek(64);
    let mut out = data.to_vec();
    cipher.apply_keystream(&mut out);
    out
}

fn e2e(code_version: i32, version: i32, trailer: bool) {
    let payload = build_payload(code_version, version, trailer);

    let a_plain = vec![0x41u8; 128];
    let b_plain: Vec<u8> = (0..512).map(|i| (i % 251) as u8).collect();
    let mut b_nonce = [0u8; 12];
    b_nonce[..4].copy_from_slice(&VFS_PROTO_VERSION.to_le_bytes());
    b_nonce[4..].copy_from_slice(&B_IV_SEED.to_le_bytes());
    let b_enc = keystream_xor(&b_nonce, &b_plain);

    let mut chk = Vec::new();
    chk.extend_from_slice(&a_plain);
    chk.extend_from_slice(&[0u8; 128]);
    chk.extend_from_slice(&b_enc);

    let blc_nonce = [0x33u8; 12];
    let blc_enc = keystream_xor(&blc_nonce, &payload);
    let mut blc = Vec::new();
    blc.extend_from_slice(&blc_nonce);
    blc.extend_from_slice(&blc_enc);

    let root = std::env::temp_dir().join(format!(
        "blc_vfs_compat_{}_{}",
        std::process::id(),
        code_version
    ));
    let pkg_dir = root.join("A1B2C3D4");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&pkg_dir).unwrap();
    fs::write(pkg_dir.join("A1B2C3D4.blc"), &blc).unwrap();
    let chunk_name: String = CHUNK_MD5
        .to_le_bytes()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect();
    fs::write(pkg_dir.join(format!("{}.chk", chunk_name)), &chk).unwrap();

    let vfs = MultiVFS::mount_folder(&root).expect("mount failed");
    assert_eq!(vfs.get_package_count(), 1);
    assert_eq!(vfs.get_total_file_count(), 2);

    let pkg = vfs
        .get_package("A1B2C3D4")
        .expect("package A1B2C3D4 missing");
    assert_eq!(pkg.read_file("a.bin").unwrap(), a_plain, "未加密直读不一致");
    assert_eq!(pkg.read_file("b.bin").unwrap(), b_plain, "加密文件解密不一致");

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn e2e_v5_mount_and_read() {
    e2e(5, LEGACY_VERSION_CONST, true);
}

#[test]
fn e2e_v4_mount_and_read() {
    e2e(4, LEGACY_VERSION_CONST, false);
}

#[test]
fn e2e_v3_mount_and_read() {
    e2e(3, LEGACY_VERSION_CONST, false);
}
