use blc_vfs::{MultiVFS, PckExtractor};
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::PathBuf;

fn main() {
    print_banner();
    
    let mut vfs: Option<MultiVFS> = None;
    let mut current_dir = String::new();
    
    loop {
        let prompt_str = if vfs.is_some() {
            format!("blc-vfs:/{}> ", current_dir)
        } else {
            "blc-vfs> ".to_string()
        };
        
        let input = prompt(&prompt_str);
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.is_empty() {
            continue;
        }
        
        let should_exit = handle_command(parts, &mut vfs, &mut current_dir);
        if should_exit {
            break;
        }
    }
}

fn handle_command(parts: Vec<&str>, vfs: &mut Option<MultiVFS>, current_dir: &mut String) -> bool {
    match parts[0] {
        "mount" => cmd_mount(parts, vfs, current_dir),
        "ls" => cmd_ls(parts, vfs, current_dir),
        "cd" => cmd_cd(parts, vfs, current_dir),
        "pwd" => cmd_pwd(vfs, current_dir),
        "extract" => cmd_extract(parts, vfs, current_dir),
        "extract-dir" => cmd_extract_dir(parts, vfs, current_dir),
        "extract-all" => cmd_extract_all(parts, vfs, current_dir),
        "extract-pck" => cmd_extract_pck(parts, vfs, current_dir),
        "extract-pck-all" => cmd_extract_pck_all(parts, vfs, current_dir),
        "extract-pck-entries" => cmd_extract_pck_entries(parts, vfs, current_dir),
        "pck-list" => cmd_pck_list(vfs),
        "packages" => cmd_packages(vfs),
        "help" => print_help(),
        "quit" | "exit" => {
            println!("Goodbye!");
            return true;
        }
        _ => {
            println!("Unknown command: {}. Type 'help' for usage.", parts[0]);
        }
    }
    false
}

fn cmd_mount(parts: Vec<&str>, vfs: &mut Option<MultiVFS>, current_dir: &mut String) {
    if parts.len() < 2 {
        println!("Usage: mount <folder>");
        return;
    }
    
    let folder = PathBuf::from(parts[1]);
    println!("Scanning folder: {}", folder.display());
    
    match MultiVFS::mount_folder(&folder) {
        Ok(mounted_vfs) => {
            println!("✓ Mount successful!");
            println!("  Packages: {}", mounted_vfs.get_package_count());
            println!("  Total files: {}", mounted_vfs.get_total_file_count());
            *vfs = Some(mounted_vfs);
            current_dir.clear();
        }
        Err(e) => {
            println!("✗ Mount failed: {}", e);
        }
    }
}

fn cmd_ls(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    match vfs {
        Some(mounted_vfs) => {
            let target_dir = if parts.len() > 1 {
                join_paths(current_dir, parts[1])
            } else {
                current_dir.clone()
            };
            
            let entries = list_directory(mounted_vfs, &target_dir);
            
            if entries.is_empty() {
                println!("(empty directory)");
            } else {
                for entry in entries {
                    let full_path = join_paths(&target_dir, &entry);
                    if is_directory(mounted_vfs, &full_path) {
                        println!("  📁 {}/", entry);
                    } else {
                        println!("  📄 {}", entry);
                    }
                }
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_cd(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &mut String) {
    match vfs {
        Some(_) => {
            if parts.len() < 2 {
                current_dir.clear();
                return;
            }
            
            let target = parts[1];
            
            if target == "/" || target == "\\" {
                current_dir.clear();
            } else if target == ".." {
                if let Some(last_slash) = current_dir.rfind('/') {
                    *current_dir = current_dir[..last_slash].to_string();
                } else {
                    current_dir.clear();
                }
            } else if target != "." {
                let new_dir = join_paths(current_dir, target);
                
                if is_directory(vfs.as_ref().unwrap(), &new_dir) {
                    *current_dir = new_dir;
                } else {
                    println!("✗ Directory not found: {}", target);
                }
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_pwd(vfs: &Option<MultiVFS>, current_dir: &String) {
    match vfs {
        Some(_) => {
            if current_dir.is_empty() {
                println!("/");
            } else {
                println!("/{}", current_dir);
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    if parts.len() < 2 {
        println!("Usage: extract <file>");
        return;
    }

    match vfs {
        Some(mounted_vfs) => {
            let file_path = if parts[1].starts_with('/') {
                parts[1][1..].to_string()
            } else {
                join_paths(current_dir, parts[1])
            };

            if !is_file(mounted_vfs, &file_path) {
                println!("✗ File not found: {}", file_path);
                return;
            }

            let output_dir = PathBuf::from("output");

            match extract_file(mounted_vfs, &file_path, &output_dir) {
                Ok(()) => println!("✓ File saved to: output/{}", file_path),
                Err(e) => println!("✗ Extract failed: {}", e),
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract_dir(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    match vfs {
        Some(mounted_vfs) => {
            // If no directory specified, use current directory
            let target_dir = if parts.len() < 2 {
                current_dir.clone()
            } else if parts[1].starts_with('/') {
                parts[1][1..].to_string()
            } else {
                join_paths(current_dir, parts[1])
            };

            let output_dir = if parts.len() > 2 {
                PathBuf::from(parts[2])
            } else {
                PathBuf::from("output")
            };

            println!("Extracting directory: /{}", target_dir);

            let all_files = mounted_vfs.list_all_files();
            let dir_prefix = format!("{}/", target_dir.trim_matches('/'));

            let files_to_extract: Vec<_> = all_files
                .iter()
                .filter(|f| {
                    if target_dir.is_empty() {
                        // Root directory: all files
                        true
                    } else {
                        f.starts_with(&dir_prefix)
                    }
                })
                .collect();

            let total = files_to_extract.len();

            if total == 0 {
                println!("No files found in directory: /{}", target_dir);
                return;
            }

            println!("Found {} files to extract", total);

            let mut success = 0;
            let mut failed = 0;

            for (i, file_path) in files_to_extract.iter().enumerate() {
                match extract_file(mounted_vfs, file_path, &output_dir) {
                    Ok(()) => success += 1,
                    Err(e) => {
                        println!("✗ Failed to extract {}: {}", file_path, e);
                        failed += 1;
                    }
                }

                if (i + 1) % 100 == 0 {
                    println!("Progress: {}/{}", i + 1, total);
                }
            }

            println!("\nExtraction complete!");
            println!("  Success: {}", success);
            println!("  Failed: {}", failed);
            println!("  Output: {}/{}", output_dir.display(), target_dir);
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract_all(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    match vfs {
        Some(mounted_vfs) => {
            let output_dir = if parts.len() > 1 {
                PathBuf::from(parts[1])
            } else {
                PathBuf::from("output")
            };
            
            println!("Extracting all files from: /{}", current_dir);
            
            let all_files = mounted_vfs.list_all_files();
            let current_prefix = if current_dir.is_empty() {
                String::new()
            } else {
                format!("{}/", current_dir.trim_matches('/'))
            };
            
            let files_to_extract: Vec<_> = all_files.iter()
                .filter(|f| current_dir.is_empty() || f.starts_with(&current_prefix))
                .collect();
            
            let total = files_to_extract.len();
            
            if total == 0 {
                println!("No files to extract in current directory");
                return;
            }
            
            let mut success = 0;
            let mut failed = 0;
            
            for (i, file_path) in files_to_extract.iter().enumerate() {
                match extract_file(mounted_vfs, file_path, &output_dir) {
                    Ok(()) => success += 1,
                    Err(e) => {
                        println!("✗ Failed to extract {}: {}", file_path, e);
                        failed += 1;
                    }
                }
                
                if (i + 1) % 100 == 0 {
                    println!("Progress: {}/{}", i + 1, total);
                }
            }
            
            println!("\nExtraction complete!");
            println!("  Success: {}", success);
            println!("  Failed: {}", failed);
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract_pck(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    if parts.len() < 2 {
        println!("Usage: extract-pck <pck_file> [output_dir]");
        println!("  Extracts a .pck file, removing VFS encryption to produce a valid AKPK PCK.");
        return;
    }

    match vfs {
        Some(mounted_vfs) => {
            let file_path = if parts[1].starts_with('/') {
                parts[1][1..].to_string()
            } else {
                join_paths(current_dir, parts[1])
            };

            if !is_file(mounted_vfs, &file_path) {
                println!("✗ File not found: {}", file_path);
                return;
            }

            let output_dir = if parts.len() > 2 {
                PathBuf::from(parts[2])
            } else {
                PathBuf::from("output")
            };

            match mounted_vfs.read_pck_file(&file_path) {
                Ok(decrypted_data) => {
                    let file_name = std::path::Path::new(&file_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("output.pck");
                    let output_path = output_dir.join(file_name);

                    if let Some(parent) = output_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            println!("✗ Failed to create output directory: {}", e);
                            return;
                        }
                    }

                    match std::fs::write(&output_path, &decrypted_data) {
                        Ok(()) => {
                            println!(
                                "✓ PCK extracted ({} bytes) -> {}",
                                decrypted_data.len(),
                                output_path.display()
                            );
                        }
                        Err(e) => println!("✗ Failed to write file: {}", e),
                    }
                }
                Err(e) => println!("✗ PCK extraction failed: {}", e),
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract_pck_all(parts: Vec<&str>, vfs: &Option<MultiVFS>, _current_dir: &String) {
    match vfs {
        Some(mounted_vfs) => {
            let output_dir = if parts.len() > 1 {
                PathBuf::from(parts[1])
            } else {
                PathBuf::from("output")
            };

            let pck_files = mounted_vfs.list_pck_files();
            let total = pck_files.len();

            if total == 0 {
                println!("No .pck files found in VFS");
                return;
            }

            println!("Found {} PCK files, extracting...", total);

            let mut success = 0;
            let mut failed = 0;

            for (i, pck_path) in pck_files.iter().enumerate() {
                match mounted_vfs.read_pck_file(pck_path) {
                    Ok(decrypted_data) => {
                        let file_name = std::path::Path::new(pck_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("output.pck");
                        let output_path = output_dir.join(file_name);

                        if let Some(parent) = output_path.parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }

                        match std::fs::write(&output_path, &decrypted_data) {
                            Ok(()) => success += 1,
                            Err(_) => failed += 1,
                        }
                    }
                    Err(_) => failed += 1,
                }

                if (i + 1) % 10 == 0 || i + 1 == total {
                    println!("Progress: {}/{}", i + 1, total);
                }
            }

            println!("\nPCK extraction complete!");
            println!("  Success: {}", success);
            println!("  Failed: {}", failed);
            println!("  Output: {}", output_dir.display());
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_extract_pck_entries(parts: Vec<&str>, vfs: &Option<MultiVFS>, current_dir: &String) {
    match vfs {
        Some(mounted_vfs) => {
            if parts.len() < 2 {
                println!("Usage: extract-pck-entries <pck-file> [output_dir]");
                println!("  Extracts individual WEM/BNK files from a PCK archive");
                return;
            }

            let pck_path = join_paths(current_dir, parts[1]);
            let output_dir = if parts.len() > 2 {
                PathBuf::from(parts[2])
            } else {
                PathBuf::from("output")
            };

            println!("Extracting PCK entries: {}", pck_path);

            let raw_data = match mounted_vfs.read_file(&pck_path) {
                Ok(data) => data,
                Err(e) => {
                    println!("✗ Failed to read PCK file: {}", e);
                    return;
                }
            };

            let pck_name = std::path::Path::new(&pck_path)
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("pck");

            match blc_vfs::PckExtractor::extract_entries(&raw_data, &output_dir, Some(pck_name)) {
                Ok(paths) => {
                    println!("✓ Successfully extracted {} entries", paths.len());
                    
                    let mut wem_count = 0;
                    let mut bnk_count = 0;
                    let mut other_count = 0;
                    
                    for path in &paths {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        match ext {
                            "wem" => wem_count += 1,
                            "bnk" => bnk_count += 1,
                            _ => other_count += 1,
                        }
                    }
                    
                    println!("  WEM files: {}", wem_count);
                    println!("  BNK files: {}", bnk_count);
                    if other_count > 0 {
                        println!("  Other files: {}", other_count);
                    }
                    println!("  Output: {}/{}", output_dir.display(), pck_name);
                }
                Err(e) => {
                    println!("✗ Failed to extract entries: {}", e);
                }
            }
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_pck_list(vfs: &Option<MultiVFS>) {
    match vfs {
        Some(mounted_vfs) => {
            let pck_files = mounted_vfs.list_pck_files();

            if pck_files.is_empty() {
                println!("No .pck files found in VFS");
                return;
            }

            println!("\nPCK files ({} total):", pck_files.len());
            println!("{}", "─".repeat(80));

            for pck_path in &pck_files {
                let raw_data = match mounted_vfs.read_file(pck_path) {
                    Ok(data) => data,
                    Err(_) => {
                        println!("  {} (failed to read)", pck_path);
                        continue;
                    }
                };

                let content = match PckExtractor::parse_pck(&raw_data) {
                    Ok(c) => c,
                    Err(_) => {
                        println!("  {} (failed to parse)", pck_path);
                        continue;
                    }
                };

                let mut wem_count = 0;
                let mut bnk_count = 0;
                let mut plg_count = 0;
                for entry in &content.entries {
                    if entry.size < 4 {
                        continue;
                    }
                    let offset = entry.offset as usize;
                    let size = entry.size as usize;
                    if offset + size > raw_data.len() {
                        continue;
                    }
                    let magic = &raw_data[offset..offset + size.min(4)];
                    match magic {
                        b"BKHD" => bnk_count += 1,
                        b"RIFF" | b"RIFX" => wem_count += 1,
                        b"PLUG" => plg_count += 1,
                        _ => {}
                    }
                }

                println!(
                    "  {} ({} entries, {} lang, WEM={}, BNK={}, PLG={})",
                    pck_path,
                    content.entries.len(),
                    content.languages.len(),
                    wem_count,
                    bnk_count,
                    plg_count
                );
            }

            println!("{}", "─".repeat(80));
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn cmd_packages(vfs: &Option<MultiVFS>) {
    match vfs {
        Some(mounted_vfs) => {
            println!("\nPackage list ({} packages):", mounted_vfs.get_package_count());
            println!("{}", "─".repeat(80));
            
            let packages = mounted_vfs.list_packages();
            for (i, package_name) in packages.iter().enumerate() {
                if let Some(package) = mounted_vfs.get_package(package_name) {
                    println!("{:3}. {} ({} files)", i + 1, package_name, package.get_file_count());
                }
            }
            
            println!("{}", "─".repeat(80));
        }
        None => {
            println!("✗ Please mount a folder first");
        }
    }
}

fn print_banner() {
    println!("╔════════════════════════════════════════╗");
    println!("║     BLC Virtual File System v1.6.0     ║");
    println!("║        Rust Implementation             ║");
    println!("╚════════════════════════════════════════╝");
    println!();
}

fn print_help() {
    println!("Available commands:");
    println!("  mount <folder>                 - Mount folder containing packages");
    println!("  ls [path]                      - List files in current or specified directory");
    println!("  cd <path>                      - Change directory");
    println!("  pwd                            - Print working directory");
    println!("  extract <file>                 - Extract single file");
    println!("  extract-dir <dir> [output]     - Extract all files in a directory");
    println!("  extract-all [output_dir]       - Extract all files");
    println!("  extract-pck <file> [output]    - Extract PCK file (decrypt VFS, output .pck)");
    println!("  extract-pck-all [output]       - Extract all PCK files from VFS");
    println!("  extract-pck-entries <file> [out] - Extract WEM/BNK files from PCK");
    println!("  pck-list                       - List all PCK files and their contents");
    println!("  packages                       - List all mounted packages");
    println!("  help                           - Show this help");
    println!("  quit / exit                    - Exit program");
    println!();
}

fn prompt(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    if path.starts_with('/') {
        path[1..].to_string()
    } else {
        path
    }
}

fn join_paths(base: &str, relative: &str) -> String {
    let base = normalize_path(base);
    let relative = normalize_path(relative);
    
    if relative.is_empty() {
        return base;
    }
    
    if relative.starts_with('/') {
        return relative;
    }
    
    if base.is_empty() {
        return relative;
    }
    
    format!("{}/{}", base.trim_end_matches('/'), relative.trim_start_matches('/'))
}

fn list_directory(vfs: &MultiVFS, current_dir: &str) -> Vec<String> {
    let all_files = vfs.list_all_files();
    let current_prefix = if current_dir.is_empty() {
        String::new()
    } else {
        format!("{}/", current_dir.trim_matches('/'))
    };
    
    let mut entries = HashSet::new();
    
    for file in all_files {
        let file_str = file.to_string();
        
        if current_dir.is_empty() || file_str.starts_with(&current_prefix) {
            let remaining = if current_dir.is_empty() {
                file_str.as_str()
            } else {
                &file_str[current_prefix.len()..]
            };
            
            if let Some(slash_pos) = remaining.find('/') {
                entries.insert(remaining[..slash_pos].to_string());
            } else if !remaining.is_empty() {
                entries.insert(remaining.to_string());
            }
        }
    }
    
    let mut result: Vec<String> = entries.into_iter().collect();
    result.sort();
    result
}

fn is_directory(vfs: &MultiVFS, path: &str) -> bool {
    let all_files = vfs.list_all_files();
    let prefix = format!("{}/", path.trim_matches('/'));
    
    all_files.iter().any(|f| f.starts_with(&prefix))
}

fn is_file(vfs: &MultiVFS, path: &str) -> bool {
    vfs.file_exists(path)
}

fn extract_file(vfs: &MultiVFS, file_path: &str, output_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let data = if file_path.to_lowercase().ends_with(".pck") {
        vfs.read_pck_file(file_path)?
    } else {
        vfs.read_file(file_path)?
    };
    let output_path = output_dir.join(file_path);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&output_path, data)?;
    println!("✓ Extracted: {}", file_path);

    Ok(())
}
