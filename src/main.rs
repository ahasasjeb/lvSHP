#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::NativeOptions;

mod app;
mod palette;
mod color_match;
mod shp;
mod image_io;

/// 程序入口：基于 eframe/egui 的桌面应用
fn main() -> eframe::Result<()> {
    let native_options = NativeOptions::default();
    eframe::run_native(
        "SHP 编辑器",
        native_options,
        Box::new(|cc| Box::new(app::MixApp::new(cc))),
    )
}


