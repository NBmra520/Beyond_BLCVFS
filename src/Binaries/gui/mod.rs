use eframe::egui;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;

mod types;
mod pck_view;
mod file_browser;
mod search_panel;
mod extraction;
mod header;
mod status_bar;
mod progress_bar;
mod packages_window;

pub use types::*;
pub use pck_view::PckViewUI;
pub use file_browser::{FileBrowserUI, list_directory};
pub use extraction::ExtractionManager;

// SearchPanelUI is defined but currently unused - kept for future use
#[allow(unused_imports)]
pub use search_panel::SearchPanelUI;

use header::HeaderUI;
use status_bar::StatusBarUI;
use progress_bar::ProgressBarUI;
use packages_window::PackagesWindowUI;

pub struct BlcVfsApp {
    vfs: Option<blc_vfs::MultiVFS>,
    current_dir: String,
    selected_file: Option<String>,
    selected_files: HashSet<String>,
    entries: Vec<FileEntry>,
    status_message: String,
    extract_progress: Option<ExtractProgress>,
    search_query: String,
    search_results: Vec<String>,
    show_search: bool,
    show_packages_window: bool,
    pending_extraction: Option<Vec<String>>,
    pck_view: Option<PckView>,
    // Manifest 虚拟目录视图
    manifest_view: Option<ManifestView>,
    manifest_entries: Vec<VirtualFileEntry>,
    selected_manifest_entries: HashSet<String>,
    // Manifest 搜索
    manifest_search_query: String,
    manifest_search_results: Vec<String>,
    show_manifest_search: bool,
    // Async extraction state
    extraction_state: Option<ExtractionState>,
    // Rebuild CAB nodes into UnityFS .ab bundles on extraction
    ab_decrypt: bool,
}

struct ExtractionState {
    files: Vec<String>,
    current_index: usize,
    success: usize,
    failed: usize,
}

impl Default for BlcVfsApp {
    fn default() -> Self {
        Self {
            vfs: None,
            current_dir: String::new(),
            selected_file: None,
            selected_files: HashSet::new(),
            entries: Vec::new(),
            status_message: "Click 'Mount' to select a VFS folder".to_string(),
            extract_progress: None,
            search_query: String::new(),
            search_results: Vec::new(),
            show_search: false,
            show_packages_window: false,
            pending_extraction: None,
            pck_view: None,
            // Manifest 虚拟目录视图
            manifest_view: None,
            manifest_entries: Vec::new(),
            selected_manifest_entries: HashSet::new(),
            // Manifest 搜索
            manifest_search_query: String::new(),
            manifest_search_results: Vec::new(),
            show_manifest_search: false,
            extraction_state: None,
            ab_decrypt: false,
        }
    }
}

impl eframe::App for BlcVfsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Progress bar at top (if extracting) - render before CentralPanel
        if self.extract_progress.is_some() {
            egui::TopBottomPanel::top("progress_panel").show(ctx, |ui| {
                if let Some(ref progress) = self.extract_progress {
                    ProgressBarUI::render(ui, progress);
                }
            });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Header with buttons
            self.render_header(ui, ctx);
            ui.separator();

            // Status bar
            self.render_status_bar(ui);

            // Search panel
            if self.show_search {
                self.render_search_panel(ctx);
            }

            // Manifest search panel
            if self.show_manifest_search {
                self.render_manifest_search_panel(ctx);
            }

            // Main content area
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(ref mut pck_view) = self.pck_view {
                    PckViewUI::render(ui, pck_view, &mut self.status_message, &self.vfs);
                } else {
                    self.render_file_browser(ui);
                }
            });
        });

        // Bottom status bar - always visible at the bottom
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            self.render_bottom_status_bar(ui);
        });

        // Process pending extraction - initialize async state
        if let Some(files) = self.pending_extraction.take() {
            let total = files.len();
            self.extract_progress = Some(ExtractProgress {
                current: 0,
                total,
                success: 0,
                failed: 0,
            });
            self.extraction_state = Some(ExtractionState {
                files,
                current_index: 0,
                success: 0,
                failed: 0,
            });
            ctx.request_repaint();
        }

        // Process multiple files per frame for better throughput while keeping UI responsive
        if let Some(ref mut state) = self.extraction_state {
            if state.current_index < state.files.len() {
                if let Some(ref vfs) = self.vfs {
                    let output_dir = PathBuf::from("output");

                    // Process up to 10 files per frame for better performance
                    const FILES_PER_FRAME: usize = 10;
                    let end_index = std::cmp::min(state.current_index + FILES_PER_FRAME, state.files.len());

                    for idx in state.current_index..end_index {
                        let file = &state.files[idx];
                        match ExtractionManager::extract_single_file(vfs, file, &output_dir, self.ab_decrypt) {
                            Ok(()) => state.success += 1,
                            Err(_) => state.failed += 1,
                        }
                    }

                    state.current_index = end_index;

                    // Update progress
                    self.extract_progress = Some(ExtractProgress {
                        current: state.current_index,
                        total: state.files.len(),
                        success: state.success,
                        failed: state.failed,
                    });

                    // Request repaint to show progress
                    ctx.request_repaint();
                }
            } else {
                // Extraction complete
                self.status_message = format!(
                    "✓ Extracted {} files, ✗ {} failed",
                    state.success, state.failed
                );
                self.extract_progress = None;
                self.extraction_state = None;
            }
        }

        // Render packages window if open
        self.render_packages_window(ctx);
    }
}

impl BlcVfsApp {
    fn render_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let has_parent_dir = !self.current_dir.is_empty();
        let is_pck_view = self.pck_view.is_some();
        let is_manifest_view = self.manifest_view.is_some();
        let manifest_has_parent = self.manifest_view.as_ref().map(|m| m.has_parent()).unwrap_or(false);

        let response = HeaderUI::render(
            ui,
            self.vfs.is_some(),
            is_pck_view,
            has_parent_dir,
            is_manifest_view,
            manifest_has_parent,
            &mut self.ab_decrypt,
        );

        // Handle header responses
        if response.mount_clicked {
            self.mount_folder(ctx);
        }

        if response.back_clicked {
            self.pck_view = None;
        }

        if response.close_manifest_clicked {
            self.manifest_view = None;
            self.manifest_entries.clear();
            self.selected_manifest_entries.clear();
        }

        if response.root_clicked {
            self.current_dir.clear();
            self.selected_files.clear();
            self.refresh_entries();
        }

        if response.up_clicked {
            if is_manifest_view {
                if let Some(ref mut manifest_view) = self.manifest_view {
                    manifest_view.go_up();
                    self.refresh_manifest_entries();
                }
            } else {
                self.go_up();
            }
        }

        if response.search_clicked {
            self.show_search = true;
        }

        if response.manifest_search_clicked {
            self.show_manifest_search = true;
        }

        if response.packages_clicked {
            self.show_packages_window = true;
        }

        // Handle manifest view extraction buttons
        if is_manifest_view {
            if response.extract_all_clicked {
                self.extract_all_manifest_files();
            }
            if response.extract_selected_clicked {
                self.extract_selected_manifest_files();
            }
        }
    }

    fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(vfs) = &self.vfs {
            let package_count = vfs.get_package_count();
            let total_file_count = vfs.get_total_file_count();
            let selected_count = self.selected_files.len();
            let is_pck_view = self.pck_view.is_some();
            let is_manifest_view = self.manifest_view.is_some();
            let manifest_current_dir = self.manifest_view.as_ref()
                .map(|m| m.current_dir.clone())
                .unwrap_or_default();
            let manifest_selected_count = self.selected_manifest_entries.len();

            let response = StatusBarUI::render(
                ui,
                package_count,
                total_file_count,
                &self.current_dir,
                selected_count,
                is_pck_view,
                is_manifest_view,
                &manifest_current_dir,
                manifest_selected_count,
            );

            // Handle status bar responses
            if response.extract_current_dir_clicked {
                self.extract_current_dir();
            }

            if response.extract_selected_clicked {
                self.extract_selected_files();
            }

            if response.clear_selection_clicked {
                self.selected_files.clear();
            }

            // Manifest 视图的处理
            if response.extract_manifest_current_dir_clicked {
                self.extract_manifest_current_dir();
            }

            if response.extract_manifest_selected_clicked {
                self.extract_manifest_selected();
            }

            if response.clear_manifest_selection_clicked {
                self.selected_manifest_entries.clear();
            }
        }
    }

    fn render_bottom_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            // Left side: File details if a file is selected
            if let Some(ref selected) = self.selected_file {
                ui.label(format!("📄 {}", selected));
                if let Some(ref vfs) = self.vfs {
                    if let Some(package) = vfs.get_file_package(selected) {
                        ui.separator();
                        ui.label(format!("Package: {}", package));
                    }
                    // Show file size
                    if let Some(size) = vfs.get_file_size(selected) {
                        ui.separator();
                        if size < 1024 {
                            ui.label(format!("Size: {} bytes", size));
                        } else if size < 1024 * 1024 {
                            ui.label(format!("Size: {:.1} KB", size as f64 / 1024.0));
                        } else {
                            ui.label(format!("Size: {:.1} MB", size as f64 / (1024.0 * 1024.0)));
                        }
                    }
                    // Show file type based on extension
                    let file_type = if selected.to_lowercase().ends_with(".pck") {
                        "WwiseSoundbankPackage"
                    } else if selected.to_lowercase().ends_with(".ab") {
                        "UnityAssetsbundle"
                    } else if selected.to_lowercase().ends_with(".bytes") {
                        "Sparkbytes"
                    } else if selected.to_lowercase().ends_with(".lua") {
                        "Xlua"
                    } else if selected.to_lowercase().ends_with("manifest.hgmmap") {
                        "Manifest (double-click to view)"
                    } else if selected.to_lowercase().ends_with(".hgmmap") {
                        "abindex"
                    } else if selected.to_lowercase().ends_with("manifest.json") {
                        "Manifest JSON (double-click to view)"
                    } else if selected.to_lowercase().ends_with(".json") {
                        "json"
                    } else {
                        "Unknown"
                    };
                    ui.separator();
                    ui.label(format!("Type: {}", file_type));
                }
            } else {
                ui.label(&self.status_message);
            }
        });
    }

    fn render_file_browser(&mut self, ui: &mut egui::Ui) {
        // 如果处于 manifest 视图模式
        if self.manifest_view.is_some() {
            self.render_manifest_browser(ui);
            return;
        }

        if self.entries.is_empty() {
            if self.vfs.is_some() {
                ui.label("(empty directory)");
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("No VFS mounted");
                    ui.label("Click 'Mount' button to select a folder containing BLC packages");
                });
            }
            return;
        }

        let response = FileBrowserUI::render(
            ui,
            &self.entries,
            &self.selected_file,
            &mut self.selected_files,
        );

        // Handle file browser responses
        if let Some(path) = response.toggle_selection {
            if self.selected_files.contains(&path) {
                self.selected_files.remove(&path);
            } else {
                self.selected_files.insert(path);
            }
        }

        if let Some(dir) = response.new_dir {
            self.current_dir = dir;
            self.refresh_entries();
        }

        if let Some(file) = response.extract_file {
            self.extract_single_file(&file);
        }

        if let Some(manifest_path) = response.view_manifest {
            self.open_manifest_view(&manifest_path);
        }

        if let Some(pck_path) = response.open_pck {
            self.open_pck_view(&pck_path);
        }

        if let Some(selected) = response.selected_file {
            self.selected_file = Some(selected);
        }
    }

    /// 渲染 Manifest 虚拟目录浏览器
    fn render_manifest_browser(&mut self, ui: &mut egui::Ui) {
        if self.manifest_entries.is_empty() {
            ui.label("(empty directory)");
            return;
        }

        ui.horizontal(|ui| {
            ui.selectable_value(&mut false, false, "Name");
            ui.selectable_value(&mut false, false, "Type");
            ui.selectable_value(&mut false, false, "Size");
        });
        ui.separator();

        let row_height = 24.0;
        let total_rows = self.manifest_entries.len();
        let entries: Vec<VirtualFileEntry> = self.manifest_entries.clone();
        let mut navigate_to: Option<String> = None;
        let mut extract_file: Option<String> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                for row_index in row_range {
                    if row_index >= entries.len() {
                        break;
                    }
                    let entry = &entries[row_index];

                    ui.horizontal(|ui| {
                        ui.set_min_height(row_height);

                        let mut is_selected = self.selected_manifest_entries.contains(&entry.full_path);
                        if ui.checkbox(&mut is_selected, "").clicked() {
                            if is_selected {
                                self.selected_manifest_entries.insert(entry.full_path.clone());
                            } else {
                                self.selected_manifest_entries.remove(&entry.full_path);
                            }
                        }

                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let text = format!("{} {}", icon, entry.name);

                        let button_response = ui.button(&text);
                        if button_response.double_clicked() {
                            if entry.is_dir {
                                navigate_to = Some(entry.name.clone());
                            } else {
                                // 导出单个虚拟文件
                                extract_file = Some(entry.full_path.clone());
                            }
                        } else if button_response.clicked() {
                            if entry.is_dir {
                                navigate_to = Some(entry.name.clone());
                            }
                        }

                        // 显示文件大小
                        if !entry.is_dir && entry.size > 0 {
                            ui.label(format!("{} bytes", entry.size));
                        }

                        if !entry.is_dir {
                            if ui.small_button("Extract").clicked() {
                                extract_file = Some(entry.full_path.clone());
                            }
                        }
                    });
                }
            });

        // 处理导航和导出（在 borrow 结束后）
        if let Some(dir) = navigate_to {
            if let Some(ref mut manifest_view) = self.manifest_view {
                manifest_view.enter_directory(&dir);
            }
            self.refresh_manifest_entries();
        }

        if let Some(file) = extract_file {
            self.extract_manifest_file(&file);
        }
    }

    /// 打开 manifest 虚拟目录视图
    fn open_manifest_view(&mut self, manifest_path: &str) {
        let start_time = std::time::Instant::now();
        
        // 从 VFS 中读取 manifest 文件内容
        let vfs_result = if let Some(ref vfs) = self.vfs {
            vfs.read_file(manifest_path)
        } else {
            self.status_message = "No VFS mounted".to_string();
            return;
        };

        let data = match vfs_result {
            Ok(d) => d,
            Err(e) => {
                self.status_message = format!("Failed to read manifest from VFS: {}", e);
                return;
            }
        };

        // 根据文件类型选择解析方式
        let manifest_result = if blc_vfs::ManifestParser::is_hgmmap_file(manifest_path) {
            blc_vfs::ManifestParser::create_vfs_from_hgmmap(&data)
        } else if blc_vfs::ManifestParser::is_json_manifest_file(manifest_path) {
            // JSON 格式需要转换为字符串
            match String::from_utf8(data) {
                Ok(json_str) => blc_vfs::ManifestParser::create_vfs(&json_str),
                Err(e) => Err(format!("Invalid UTF-8 in JSON manifest: {}", e)),
            }
        } else {
            Err(format!("Unknown manifest format: {}", manifest_path))
        };

        match manifest_result {
            Ok(vfs) => {
                // 获取 hgmmap 目录路径 - 使用 VFS 中文件的实际 package 路径
                let hgmmap_path = if let Some(ref multi_vfs) = self.vfs {
                    if let Some(package_path) = multi_vfs.get_file_package_path(manifest_path) {
                        // package_path 是 .../Data/Bundles/Windows/
                        // hgmmap 应该在 .../Data/hgmmap/
                        if let Some(parent) = package_path.parent() {
                            if let Some(data_dir) = parent.parent() {
                                data_dir.join("hgmmap").to_string_lossy().to_string()
                            } else {
                                package_path.to_string_lossy().to_string()
                            }
                        } else {
                            package_path.to_string_lossy().to_string()
                        }
                    } else {
                        blc_vfs::ManifestParser::get_hgmmap_dir(manifest_path)
                            .unwrap_or_default()
                    }
                } else {
                    blc_vfs::ManifestParser::get_hgmmap_dir(manifest_path)
                        .unwrap_or_default()
                };

                println!("[DEBUG] open_manifest_view: hgmmap_path='{}'", hgmmap_path);
                
                // 计算 AB 文件在 VFS 中的前缀路径
                // manifest 路径如: Data/Bundles/Windows/manifest.hgmmap
                // AB 文件前缀应该是: Data/Bundles/Windows/
                let ab_file_prefix = std::path::Path::new(manifest_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                println!("[DEBUG] open_manifest_view: ab_file_prefix='{}'", ab_file_prefix);
                
                let manifest_view = ManifestView::new(
                    manifest_path.to_string(),
                    vfs,
                    hgmmap_path,
                    ab_file_prefix,
                );

                // 加载初始条目
                self.manifest_entries = manifest_view.list_current_entries();
                self.manifest_entries.sort_by(|a, b| {
                    b.is_dir.cmp(&a.is_dir)
                        .then_with(|| a.name.cmp(&b.name))
                });

                self.manifest_view = Some(manifest_view);
                
                let total_time = start_time.elapsed();
                println!("[DEBUG] open_manifest_view: total time {:?}", total_time);
                self.status_message = format!("Opened manifest view: {} (loaded in {:?})", manifest_path, total_time);
            }
            Err(e) => {
                self.status_message = format!("Failed to parse manifest: {}", e);
            }
        }
    }

    /// 刷新 manifest 条目列表
    fn refresh_manifest_entries(&mut self) {
        if let Some(ref manifest_view) = self.manifest_view {
            self.manifest_entries = manifest_view.list_current_entries();
            self.manifest_entries.sort_by(|a, b| {
                b.is_dir.cmp(&a.is_dir)
                    .then_with(|| a.name.cmp(&b.name))
            });
        }
    }

    /// 从 manifest 视图导出单个文件
    fn extract_manifest_file(&mut self, virtual_path: &str) {
        println!("[DEBUG] extract_manifest_file: virtual_path='{}'", virtual_path);
        if let Some(ref manifest_view) = self.manifest_view {
            // 获取 AB 文件在 VFS 中的路径
            let ab_vfs_path = match manifest_view.get_ab_vfs_path(virtual_path) {
                Some(path) => path,
                None => {
                    println!("[DEBUG] extract_manifest_file: AB VFS path not found for '{}'", virtual_path);
                    self.status_message = format!("AB file not found for: {}", virtual_path);
                    return;
                }
            };
            println!("[DEBUG] extract_manifest_file: ab_vfs_path='{}'", ab_vfs_path);
            
            // 从 VFS 中读取 AB 文件
            let ab_data = match self.vfs {
                Some(ref vfs) => {
                    match vfs.read_file(&ab_vfs_path) {
                        Ok(data) => data,
                        Err(e) => {
                            self.status_message = format!("✗ Failed to read AB file from VFS '{}': {}", ab_vfs_path, e);
                            return;
                        }
                    }
                }
                None => {
                    self.status_message = "No VFS mounted".to_string();
                    return;
                }
            };
            
            // 构建输出路径，保留虚拟路径结构
            let output_dir = std::path::PathBuf::from("output");
            let virtual_parent = std::path::Path::new(virtual_path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty());

            // 从虚拟路径获取 asset 路径（去掉 .ab 后缀）
            let _asset_path = virtual_path.strip_suffix(".ab").unwrap_or(virtual_path);

            if self.ab_decrypt && blc_vfs::is_bundle_node(&ab_vfs_path) {
                match blc_vfs::export_bundles(&ab_data, &ab_vfs_path, true) {
                    Ok(files) => {
                        let mut exported = Vec::new();
                        for file in files {
                            let mut output_path = output_dir.clone();
                            if let Some(dir) = virtual_parent {
                                output_path.push(dir);
                            }
                            output_path.push(&file.name);
                            if let Some(parent) = output_path.parent() {
                                if let Err(e) = std::fs::create_dir_all(parent) {
                                    self.status_message = format!("Failed to create directory: {}", e);
                                    return;
                                }
                            }
                            match std::fs::write(&output_path, &file.bytes) {
                                Ok(_) => exported.push(output_path),
                                Err(e) => {
                                    self.status_message = format!("✗ Failed to write: {}", e);
                                    return;
                                }
                            }
                        }
                        self.status_message = format!(
                            "✓ Exported (decrypted): {} -> {}",
                            virtual_path,
                            exported
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        return;
                    }
                    Err(e) => {
                        println!(
                            "[DEBUG] extract_manifest_file: AB decrypt failed for '{}': {} (writing raw)",
                            virtual_path, e
                        );
                    }
                }
            }

            let output_path = output_dir.join(virtual_path);

            // 确保父目录存在
            if let Some(parent) = output_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    self.status_message = format!("Failed to create directory: {}", e);
                    return;
                }
            }

            // 未解密（或解密失败）：原样写出 AB 容器字节
            match std::fs::write(&output_path, ab_data) {
                Ok(_) => {
                    self.status_message = format!("✓ Exported: {} -> {}", virtual_path, output_path.display());
                }
                Err(e) => {
                    self.status_message = format!("✗ Failed to write: {}", e);
                }
            }
        }
    }

    /// 导出当前 manifest 视图中的所有文件（包括子目录）
    fn extract_all_manifest_files(&mut self) {
        if let Some(ref manifest_view) = self.manifest_view {
            // 获取当前目录及其子目录下的所有文件
            let files = manifest_view.vfs.get_all_files_in_dir(&manifest_view.current_dir);

            if files.is_empty() {
                self.status_message = "No files to extract in current directory".to_string();
                return;
            }

            println!("[DEBUG] extract_all_manifest_files: found {} files in '{}'", files.len(), manifest_view.current_dir);

            let count = files.len();
            for file in &files {
                self.extract_manifest_file(file);
            }
            self.status_message = format!("✓ Extracted {} files", count);
        }
    }

    /// 导出选中的 manifest 文件
    fn extract_selected_manifest_files(&mut self) {
        if self.selected_manifest_entries.is_empty() {
            self.status_message = "No files selected".to_string();
            return;
        }

        // 收集所有要导出的文件（包括选中目录下的所有文件）
        let mut files_to_extract: Vec<String> = Vec::new();
        
        if let Some(ref manifest_view) = self.manifest_view {
            for selected in &self.selected_manifest_entries {
                // 检查是否是目录
                let is_dir = manifest_view.vfs.list_directory(selected).iter().any(|e| e.is_dir) 
                    || manifest_view.vfs.get_all_files_in_dir(selected).len() > 0;
                
                if is_dir {
                    // 如果是目录，获取该目录及其子目录下的所有文件
                    let files_in_dir = manifest_view.vfs.get_all_files_in_dir(selected);
                    println!("[DEBUG] extract_selected: directory '{}' contains {} files", selected, files_in_dir.len());
                    files_to_extract.extend(files_in_dir);
                } else {
                    // 如果是文件，直接添加
                    files_to_extract.push(selected.clone());
                }
            }
        }

        // 去重
        files_to_extract.sort();
        files_to_extract.dedup();

        if files_to_extract.is_empty() {
            self.status_message = "No files to extract from selection".to_string();
            return;
        }

        let count = files_to_extract.len();
        println!("[DEBUG] extract_selected: extracting {} unique files", count);
        
        for file in &files_to_extract {
            self.extract_manifest_file(file);
        }
        self.status_message = format!("✓ Extracted {} files from selection", count);
    }

    fn render_search_panel(&mut self, ctx: &egui::Context) {
        let mut show = self.show_search;
        egui::Window::new("Search Files")
            .open(&mut show)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Query:");
                    let text_edit = ui.text_edit_singleline(&mut self.search_query);
                    if text_edit.changed() && !self.search_query.is_empty() {
                        self.perform_search();
                    }
                    if ui.button("Search").clicked() {
                        self.perform_search();
                    }
                });

                ui.separator();

                // Use virtualized rendering for search results
                let row_height = 24.0;
                let total_rows = self.search_results.len();
                let search_results = self.search_results.clone();

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show_rows(ui, row_height, total_rows, |ui, row_range| {
                        for row_index in row_range {
                            if row_index >= search_results.len() {
                                break;
                            }
                            let result = &search_results[row_index];

                            ui.horizontal(|ui| {
                                ui.set_min_height(row_height);
                                ui.label("📄");
                                if ui.selectable_label(false, result).clicked() {
                                    self.navigate_to_file(result);
                                }
                                if ui.small_button("Extract").clicked() {
                                    self.extract_single_file(result);
                                }
                            });
                        }
                    });

                if !self.search_results.is_empty() {
                    ui.separator();
                    ui.label(format!("Found {} results", self.search_results.len()));
                    if ui.button("Extract All Results").clicked() {
                        self.pending_extraction = Some(self.search_results.clone());
                        self.status_message = format!("Queued {} files for extraction", self.pending_extraction.as_ref().unwrap().len());
                    }
                }
            });

        self.show_search = show;
    }

    fn render_manifest_search_panel(&mut self, ctx: &egui::Context) {
        let mut show = self.show_manifest_search;
        egui::Window::new("Search Manifest Files")
            .open(&mut show)
            .default_width(600.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Query:");
                    ui.add_sized(
                        egui::vec2(400.0, 24.0),
                        egui::TextEdit::singleline(&mut self.manifest_search_query)
                    );
                    if ui.button("Search").clicked() {
                        self.perform_manifest_search();
                    }
                });

                ui.separator();

                // Use virtualized rendering for search results
                let row_height = 24.0;
                let total_rows = self.manifest_search_results.len();
                let search_results = self.manifest_search_results.clone();

                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show_rows(ui, row_height, total_rows, |ui, row_range| {
                        for row_index in row_range {
                            if row_index >= search_results.len() {
                                break;
                            }
                            let result = &search_results[row_index];

                            ui.horizontal(|ui| {
                                ui.set_min_height(row_height);
                                ui.label("📄");
                                if ui.selectable_label(false, result).clicked() {
                                    self.navigate_to_manifest_file(result);
                                }
                                if ui.small_button("Extract").clicked() {
                                    self.extract_manifest_file(result);
                                }
                            });
                        }
                    });

                if !self.manifest_search_results.is_empty() {
                    ui.separator();
                    ui.label(format!("Found {} results", self.manifest_search_results.len()));
                    if ui.button("Extract All Results").clicked() {
                        let results_to_extract = self.manifest_search_results.clone();
                        for result in results_to_extract {
                            self.extract_manifest_file(&result);
                        }
                        self.status_message = format!("✓ Extracted {} files from search results", self.manifest_search_results.len());
                    }
                }
            });

        self.show_manifest_search = show;
    }

    fn render_packages_window(&mut self, ctx: &egui::Context) {
        if !self.show_packages_window {
            return;
        }

        // Collect package info
        let packages: Vec<(String, usize)> = if let Some(vfs) = &self.vfs {
            vfs.get_package_files()
                .into_iter()
                .map(|(name, count)| (name, count))
                .collect()
        } else {
            Vec::new()
        };

        let response = PackagesWindowUI::render(
            ctx,
            &mut self.show_packages_window,
            &packages,
        );

        if response.should_close {
            self.show_packages_window = false;
        }

        // Handle package unloading
        for package_name in response.packages_to_unload {
            if let Some(vfs) = &mut self.vfs {
                match vfs.unload_package(&package_name) {
                    Ok(()) => {
                        self.status_message = format!("✓ Unloaded package '{}'", package_name);
                        self.refresh_entries();
                    }
                    Err(e) => {
                        self.status_message = format!("✗ Failed to unload '{}': {}", package_name, e);
                    }
                }
            }
        }

        // Check if all packages have been unloaded
        if let Some(vfs) = &self.vfs {
            if vfs.get_package_count() == 0 {
                self.vfs = None;
                self.current_dir.clear();
                self.selected_files.clear();
                self.entries.clear();
                self.show_packages_window = false;
                self.status_message = "All packages unloaded. Click 'Mount' to select a VFS folder".to_string();
            }
        }
    }

    // Helper methods
    fn mount_folder(&mut self, ctx: &egui::Context) {
        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
            self.status_message = format!("Mounting: {}", folder.display());
            ctx.request_repaint();

            match blc_vfs::MultiVFS::mount_folder(&folder) {
                Ok(vfs) => {
                    self.status_message = format!(
                        "✓ Mounted {} packages, {} files",
                        vfs.get_package_count(),
                        vfs.get_total_file_count()
                    );
                    self.vfs = Some(vfs);
                    self.current_dir.clear();
                    self.selected_files.clear();
                    self.refresh_entries();
                }
                Err(e) => {
                    self.status_message = format!("✗ Mount failed: {}", e);
                }
            }
        }
    }

    fn refresh_entries(&mut self) {
        if let Some(vfs) = &self.vfs {
            self.entries = list_directory(vfs, &self.current_dir);
            self.entries.sort_by(|a, b| {
                b.is_dir.cmp(&a.is_dir)
                    .then_with(|| a.name.cmp(&b.name))
            });
        }
    }

    fn go_up(&mut self) {
        if let Some(last_slash) = self.current_dir.rfind('/') {
            self.current_dir = self.current_dir[..last_slash].to_string();
        } else {
            self.current_dir.clear();
        }
        self.refresh_entries();
    }

    fn extract_single_file(&mut self, file_path: &str) {
        if let Some(vfs) = &self.vfs {
            let output_dir = PathBuf::from("output");

            match ExtractionManager::extract_single_file(vfs, file_path, &output_dir, self.ab_decrypt) {
                Ok(()) => {
                    self.status_message = format!("✓ Extracted: {}", file_path);
                }
                Err(e) => {
                    self.status_message = format!("✗ Failed: {}", e);
                }
            }
        }
    }

    fn extract_current_dir(&mut self) {
        if let Some(vfs) = &self.vfs {
            let files_to_extract = ExtractionManager::get_files_in_current_dir(vfs, &self.current_dir);

            println!("[DEBUG] Files to extract: {}", files_to_extract.len());
            self.pending_extraction = Some(files_to_extract);
            self.status_message = format!("Queued {} files for extraction", self.pending_extraction.as_ref().unwrap().len());
        }
    }

    fn extract_selected_files(&mut self) {
        if let Some(vfs) = &self.vfs {
            let files_to_extract = ExtractionManager::get_selected_files(vfs, &self.selected_files);

            self.pending_extraction = Some(files_to_extract);
            self.status_message = format!("Queued {} files for extraction", self.pending_extraction.as_ref().unwrap().len());
        }
    }

    fn perform_search(&mut self) {
        if let Some(vfs) = &self.vfs {
            let query = self.search_query.to_lowercase();
            const MAX_RESULTS: usize = 1000;

            // Use parallel processing for large file lists
            let all_files: Vec<String> = vfs.list_all_files()
                .into_iter()
                .map(|s| s.to_string())
                .collect();

            if all_files.len() > 10000 {
                // Parallel search for large datasets
                let results: Vec<String> = all_files
                    .par_iter()
                    .filter(|f| f.to_lowercase().contains(&query))
                    .map(|s| s.clone())
                    .collect();
                self.search_results = results.into_iter().take(MAX_RESULTS).collect();
            } else {
                // Sequential search for smaller datasets
                self.search_results = all_files
                    .into_iter()
                    .filter(|f| f.to_lowercase().contains(&query))
                    .take(MAX_RESULTS)
                    .collect();
            }
        }
    }

    fn navigate_to_file(&mut self, file_path: &str) {
        if let Some(last_slash) = file_path.rfind('/') {
            self.current_dir = file_path[..last_slash].to_string();
            self.selected_file = Some(file_path.to_string());
            self.refresh_entries();
        }
    }

    fn perform_manifest_search(&mut self) {
        if let Some(ref manifest_view) = self.manifest_view {
            let query = self.manifest_search_query.to_lowercase();

            // Get all virtual paths from manifest
            let all_files = manifest_view.vfs.get_all_virtual_paths();

            // Search for matching files (no limit)
            self.manifest_search_results = all_files
                .into_iter()
                .filter(|f| f.to_lowercase().contains(&query))
                .collect();
        }
    }

    fn navigate_to_manifest_file(&mut self, file_path: &str) {
        if let Some(ref mut manifest_view) = self.manifest_view {
            if let Some(last_slash) = file_path.rfind('/') {
                manifest_view.current_dir = file_path[..last_slash].to_string();
                self.refresh_manifest_entries();
                self.show_manifest_search = false;
            }
        }
    }

    /// 导出 manifest 当前目录及其子目录下的所有虚拟文件
    fn extract_manifest_current_dir(&mut self) {
        self.extract_all_manifest_files();
    }

    /// 导出选中的 manifest 虚拟文件
    fn extract_manifest_selected(&mut self) {
        self.extract_selected_manifest_files();
    }

    fn open_pck_view(&mut self, pck_path: &str) {
        if let Some(vfs) = &self.vfs {
            match vfs.read_file(pck_path) {
                Ok(data) => {
                    match blc_vfs::PckExtractor::extract_pck(&data) {
                        Ok(result) => {
                            let entries: Vec<PckEntryView> = result.entries.iter().map(|e| {
                                let entry_type = match e.entry_type {
                                    blc_vfs::VFS::FileType::PCK::pck_extractor::PckEntryType::Wem => "WEM".to_string(),
                                    blc_vfs::VFS::FileType::PCK::pck_extractor::PckEntryType::WemX => "WEM".to_string(),
                                    blc_vfs::VFS::FileType::PCK::pck_extractor::PckEntryType::Bnk => "BNK".to_string(),
                                    blc_vfs::VFS::FileType::PCK::pck_extractor::PckEntryType::Plg => "PLG".to_string(),
                                    blc_vfs::VFS::FileType::PCK::pck_extractor::PckEntryType::Unknown => "UNKNOWN".to_string(),
                                };
                                PckEntryView {
                                    file_id: e.file_id,
                                    entry_type,
                                    size: e.data.len(),
                                }
                            }).collect();

                            let parent_dir = if let Some(last_slash) = pck_path.rfind('/') {
                                pck_path[..last_slash].to_string()
                            } else {
                                String::new()
                            };

                            self.pck_view = Some(PckView {
                                pck_path: pck_path.to_string(),
                                entries,
                                parent_dir,
                                selected_entries: HashSet::new(),
                            });
                        }
                        Err(e) => {
                            self.status_message = format!("✗ Failed to parse PCK: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.status_message = format!("✗ Failed to read PCK: {}", e);
                }
            }
        }
    }
}

pub fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "BLC VFS Explorer",
        options,
        Box::new(|_cc| Ok(Box::new(BlcVfsApp::default()))),
    )
}
