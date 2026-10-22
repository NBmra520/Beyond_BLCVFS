use blc_vfs::MultiVFS;
use std::path::Path;

fn head_hex(data: &[u8], n: usize) -> String {
    data.iter()
        .take(n)
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
    let folder = std::env::args().nth(1).expect("usage: vfs_smoke <folder>");
    let vfs = MultiVFS::mount_folder(Path::new(&folder)).expect("mount failed");
    println!("packages={} total_files={}", vfs.get_package_count(), vfs.get_total_file_count());

    let mut ok = 0u32;
    let mut err = 0u32;
    for pkg in vfs.list_packages() {
        let p = match vfs.get_package(pkg) {
            Some(p) => p,
            None => continue,
        };
        let mut files: Vec<&str> = p.list_files();
        files.sort();
        let samples: Vec<&str> = files.iter().take(3).copied().collect();
        println!("-- package {}: {} files | 样例:", pkg, files.len());
        for f in &samples {
            match p.read_file(f) {
                Ok(data) => {
                    let printable = data
                        .iter()
                        .take(24)
                        .map(|&b| if b >= 32 && b < 127 { b as char } else { '.' })
                        .collect::<String>();
                    println!(
                        "   [OK] {} len={} head={} ascii={:?}",
                        f,
                        data.len(),
                        head_hex(&data, 8),
                        printable
                    );
                    ok += 1;
                }
                Err(e) => {
                    println!("   [ERR] {} : {:?}", f, e);
                    err += 1;
                }
            }
        }
    }
    println!("=== summary: read_ok={} read_err={} ===", ok, err);
}
