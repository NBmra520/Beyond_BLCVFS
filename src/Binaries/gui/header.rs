use eframe::egui;

pub struct HeaderUI;

pub struct HeaderResponse {
    pub mount_clicked: bool,
    pub back_clicked: bool,
    pub root_clicked: bool,
    pub up_clicked: bool,
    pub search_clicked: bool,
    pub packages_clicked: bool,
    pub close_manifest_clicked: bool,
    pub extract_all_clicked: bool,
    pub extract_selected_clicked: bool,
    pub manifest_search_clicked: bool,
}

impl HeaderUI {
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        ui: &mut egui::Ui,
        has_vfs: bool,
        is_pck_view: bool,
        has_parent_dir: bool,
        is_manifest_view: bool,
        manifest_has_parent: bool,
        ab_decrypt: &mut bool,
    ) -> HeaderResponse {
        let mut response = HeaderResponse {
            mount_clicked: false,
            back_clicked: false,
            root_clicked: false,
            up_clicked: false,
            search_clicked: false,
            packages_clicked: false,
            close_manifest_clicked: false,
            extract_all_clicked: false,
            extract_selected_clicked: false,
            manifest_search_clicked: false,
        };

        ui.horizontal(|ui| {
            ui.heading("BLC Virtual File System v1.61");
            ui.separator();

            if ui.button("📁 Mount").clicked() {
                response.mount_clicked = true;
            }

            if has_vfs {
                if is_pck_view {
                    if ui.button("⬅ Back to VFS").clicked() {
                        response.back_clicked = true;
                    }
                } else if is_manifest_view {
                    // Manifest 视图特有的导航按钮
                    if ui.button("❌ Close Manifest").clicked() {
                        response.close_manifest_clicked = true;
                    }

                    if manifest_has_parent {
                        if ui.button("⬆ Up").clicked() {
                            response.up_clicked = true;
                        }
                    }

                    if ui.button("🔍 Search").clicked() {
                        response.manifest_search_clicked = true;
                    }
                } else {
                    if ui.button("🏠 Root").clicked() {
                        response.root_clicked = true;
                    }

                    if has_parent_dir {
                        if ui.button("⬆ Up").clicked() {
                            response.up_clicked = true;
                        }
                    }
                }

                if !is_manifest_view {
                    if ui.button("🔍 Search").clicked() {
                        response.search_clicked = true;
                    }

                    if ui.button("📦 Packages").clicked() {
                        response.packages_clicked = true;
                    }
                }

                ui.separator();
                ui.checkbox(ab_decrypt, "Decrypt AB")
                    .on_hover_text(
                        "Checked: decrypt the Endfield container and rebuild each bundle as UnityFS .ab\n\
                         NOTE: rebuilding strips Endfield-only fields, so some information is lost.\n\
                         Unchecked: export the raw VFS bytes without decrypting — for the complete\n\
                         bundle, load these in AnimeStudio with game type ArknightsEndfield.",
                    );
            }
        });

        response
    }
}
