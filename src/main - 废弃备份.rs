use eframe::egui::{self, Color32, Context, FontData, FontDefinitions, FontFamily, Key, Layout, RichText, Separator, TextEdit, Vec2};
use eframe::{self};
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::env;
mod mix;
use mix::{MixFile, format_size};

#[derive(Clone, Debug)]
struct EntryRow {
    name: String,
    is_dir: bool,
}

struct AppState {
    drives: Vec<String>,
    selected_path: Option<PathBuf>,
    entries: Vec<EntryRow>,
    error: Option<String>,
    path_input: String,
    // 右侧预览
    mix_preview: Option<MixFile>,
    mix_search: String,
    in_mix_mode: bool,
}

impl AppState {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 暗色主题 + 中文字体
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        install_cjk_font(&cc.egui_ctx);

        let drives = enumerate_drives();
        let mut selected_path: Option<PathBuf> = None;
        let mut entries: Vec<EntryRow> = Vec::new();
        let mut error: Option<String> = None;

        if let Some(first) = drives.first() {
            let path = PathBuf::from(format!("{}\\", first));
            let (rows, err) = load_entries_for_path(&path);
            selected_path = Some(path);
            entries = rows;
            error = err;
        }

        let path_input = selected_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "".to_string());

        Self { drives, selected_path, entries, error, path_input, mix_preview: None, mix_search: String::new(), in_mix_mode: false }
    }
}

fn install_cjk_font(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    // 1) 项目根目录自带字体优先
    let local_candidates: [&str; 2] = [
        "wqy-microhei.ttc",
        "wqy-microhei.ttf",
    ];
    for p in local_candidates {
        if let Ok(bytes) = fs::read(p) {
            fonts.font_data.insert("cjk".to_owned(), FontData::from_owned(bytes));
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .get_mut(&FontFamily::Monospace)
                .unwrap()
                .insert(0, "cjk".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }

    // 2) 绝对路径（当前工作目录）尝试
    if let Ok(cwd) = env::current_dir() {
        let p = cwd.join("wqy-microhei.ttc");
        if let Ok(bytes) = fs::read(&p) {
            fonts.font_data.insert("cjk".to_owned(), FontData::from_owned(bytes));
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .get_mut(&FontFamily::Monospace)
                .unwrap()
                .insert(0, "cjk".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }

    // 3) Windows 系统字体候选
    let candidates: [&str; 8] = [
        "C\\\\Windows\\\\Fonts\\\\msyh.ttc",
        "C\\\\Windows\\\\Fonts\\\\msyh.ttf",
        "C\\\\Windows\\\\Fonts\\\\msyhbd.ttc",
        "C\\\\Windows\\\\Fonts\\\\simhei.ttf",
        "C\\\\Windows\\\\Fonts\\\\simhei.ttf",
        "C\\\\Windows\\\\Fonts\\\\simsun.ttc",
        "C\\\\Windows\\\\Fonts\\\\Microsoft YaHei UI.ttf",
        "C\\\\Windows\\\\Fonts\\\\Microsoft YaHei.ttf",
    ];

    for p in candidates {
        if let Ok(bytes) = fs::read(p) {
            fonts.font_data.insert("cjk".to_owned(), FontData::from_owned(bytes));
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "cjk".to_owned());
            fonts
                .families
                .get_mut(&FontFamily::Monospace)
                .unwrap()
                .insert(0, "cjk".to_owned());
            ctx.set_fonts(fonts);
            return;
        }
    }
}

fn enumerate_drives() -> Vec<String> {
    let mut drives: Vec<String> = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let root = format!("{}\\", drive);
        if Path::new(&root).exists() {
            drives.push(drive);
        }
    }
    drives
}

fn load_entries_for_path(path: &Path) -> (Vec<EntryRow>, Option<String>) {
    let mut rows: Vec<EntryRow> = Vec::new();
    let mut error: Option<String> = None;

    match fs::read_dir(path) {
        Ok(read_dir) => {
            for entry_result in read_dir {
                match entry_result {
                    Ok(entry) => {
                        let path: PathBuf = entry.path();
                        let is_dir: bool = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
                        let name: String = match path.file_name() {
                            Some(os) => os.to_string_lossy().to_string(),
                            None => path.to_string_lossy().to_string(),
                        };
                        rows.push(EntryRow { name, is_dir });
                    }
                    Err(e) => {
                        error = Some(format!("读取目录项出错: {}", e));
                        break;
                    }
                }
            }
            rows.sort_by(|a, b| match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
        }
        Err(e) => {
            error = Some(format!("无法读取 {}: {}", path.display(), e));
        }
    }
    (rows, error)
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 顶层容器，深色风格
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("mixBrowser").strong().color(Color32::from_rgb(180, 220, 255)));
                ui.add_space(8.0);
                ui.label(RichText::new("C盘 / D盘 文件浏览器（预览版）").color(Color32::from_gray(180)));
            });
        });

        // 左侧面板：所有盘符
        egui::SidePanel::left("left_drives").resizable(true).default_width(160.0).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(RichText::new("盘符").color(Color32::from_rgb(200, 220, 255)));
            });
            ui.add_space(4.0);
            ui.add(Separator::default());
            ui.add_space(8.0);
            if ui.button("刷新盘符").clicked() {
                self.drives = enumerate_drives();
                if self.selected_path.is_none() {
                    if let Some(first) = self.drives.first() {
                        self.selected_path = Some(PathBuf::from(format!("{}\\", first)));
                    }
                }
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                for drive in &self.drives {
                    let is_selected = match &self.selected_path {
                        Some(p) => p.starts_with(&format!("{}\\", drive)),
                        None => false,
                    };
                    let label = format!("{}\\", drive);
                    let response = ui.selectable_label(is_selected, label.clone());
                    if response.clicked() {
                        let root = PathBuf::from(label);
                        let (rows, err) = load_entries_for_path(&root);
                        self.selected_path = Some(root);
                        self.entries = rows;
                        self.error = err;
                    }
                }
            });
        });

        // 中间面板：目录浏览 / MIX 浏览
        egui::SidePanel::left("middle_explorer").resizable(true).default_width(520.0).show(ctx, |ui| {
            // 路径输入与操作
            ui.horizontal(|ui| {
                let resp = ui.add(
                    TextEdit::singleline(&mut self.path_input)
                        .desired_width(f32::INFINITY)
                        .hint_text("粘贴/输入路径或 .mix 文件，按回车或点跳转")
                );
                let mut do_jump = false;
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    do_jump = true;
                }
                if ui.button("跳转").clicked() {
                    do_jump = true;
                }
                if do_jump {
                    let mut target = self.path_input.trim().to_string();
                    if target.ends_with(':') { target.push('\\'); }
                    let candidate = PathBuf::from(&target);
                    if candidate.exists() {
                        if candidate.is_dir() {
                            let (rows, err) = load_entries_for_path(&candidate);
                            self.selected_path = Some(candidate.clone());
                            self.entries = rows;
                            self.error = err;
                            self.in_mix_mode = false;
                        } else if target.to_lowercase().ends_with(".mix") {
                            match MixFile::open(&candidate) {
                                Ok(mix) => {
                                    self.mix_preview = Some(mix);
                                    self.in_mix_mode = true;
                                    self.error = None;
                                }
                                Err(e) => {
                                    self.mix_preview = None;
                                    self.in_mix_mode = false;
                                    self.error = Some(format!("打开 MIX 失败: {}", e));
                                }
                            }
                        } else {
                            self.error = Some("目标不是目录或 .mix 文件".to_string());
                        }
                    } else {
                        self.error = Some(format!("路径无效或不可访问: {}", target));
                    }
                }
            });
            ui.add_space(6.0);

            if self.in_mix_mode {
                ui.horizontal(|ui| {
                    if ui.button("退出MIX").clicked() {
                        // 回到 MIX 所在目录
                        if let Some(mix) = &self.mix_preview {
                            if let Some(parent) = mix.path.parent() {
                                let parent_buf = parent.to_path_buf();
                                let (rows, err) = load_entries_for_path(&parent_buf);
                                self.selected_path = Some(parent_buf.clone());
                                self.entries = rows;
                                self.error = err;
                                self.path_input = parent_buf.display().to_string();
                            }
                        }
                        self.in_mix_mode = false;
                    }
                    if ui.button("刷新MIX").clicked() {
                        if let Some(mix) = &self.mix_preview {
                            match MixFile::open(&mix.path) {
                                Ok(newmix) => { self.mix_preview = Some(newmix); self.error = None; }
                                Err(e) => { self.error = Some(format!("刷新失败: {}", e)); }
                            }
                        }
                    }
                });
                ui.add_space(6.0);
                if let Some(mix) = &self.mix_preview {
                    ui.label(RichText::new(format!("MIX 当前位置: {}", mix.path.display())).italics().color(Color32::from_gray(170)));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("筛选(ID十六进制片段)：");
                        let _ = ui.add(TextEdit::singleline(&mut self.mix_search));
                        if ui.button("清空").clicked() { self.mix_search.clear(); }
                    });
                    ui.add_space(4.0);
                }
                ui.add(Separator::default());
                ui.add_space(6.0);

                if let Some(err) = &self.error { ui.colored_label(Color32::LIGHT_RED, err); ui.add_space(6.0); }

                egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                    if let Some(mix) = &self.mix_preview {
                        let results = mix.search(&self.mix_search);
                        for e in results {
                            ui.label(format!("id={:08X}  offset={}  size={}", e.id, e.offset, e.size));
                        }
                    }
                });
                return; // MIX 模式已绘制完成
            }

            // 非 MIX：文件系统浏览
            ui.horizontal(|ui| {
                if ui.button("返回上级").clicked() {
                    if let Some(current) = self.selected_path.clone() {
                        if let Some(parent) = current.parent() {
                            let parent_buf = parent.to_path_buf();
                            if parent_buf.as_os_str().is_empty() {
                            } else {
                                let (rows, err) = load_entries_for_path(&parent_buf);
                                self.selected_path = Some(parent_buf);
                                self.entries = rows;
                                self.error = err;
                                if let Some(p) = &self.selected_path { self.path_input = p.display().to_string(); }
                            }
                        }
                    }
                }
                if ui.button("刷新").clicked() {
                    if let Some(current) = &self.selected_path {
                        let (rows, err) = load_entries_for_path(current);
                        self.entries = rows;
                        self.error = err;
                        if let Some(p) = &self.selected_path { self.path_input = p.display().to_string(); }
                    }
                }
            });
            ui.add_space(6.0);
            if let Some(path) = &self.selected_path {
                ui.label(RichText::new(format!("当前位置: {}", path.display())).italics().color(Color32::from_gray(170)));
            } else {
                ui.label(RichText::new("未选择路径").italics().color(Color32::from_gray(170)));
            }
            ui.add_space(4.0);
            ui.add(Separator::default());
            ui.add_space(6.0);

            if let Some(err) = &self.error { ui.colored_label(Color32::LIGHT_RED, err); ui.add_space(6.0); }

            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                let items: Vec<(String, bool)> = self
                    .entries
                    .iter()
                    .map(|r| (r.name.clone(), r.is_dir))
                    .collect();
                for (name, is_dir) in items {
                    let icon = if is_dir { "📁" } else { "📄" };
                    let label = format!("{} {}", icon, name);
                    let response = ui.selectable_label(false, label);
                    if response.clicked() {
                        if is_dir {
                            if let Some(base) = &self.selected_path {
                                let next = base.join(&name);
                                let (rows, err) = load_entries_for_path(&next);
                                if err.is_none() {
                                    self.selected_path = Some(next);
                                    self.entries = rows;
                                    self.error = None;
                                    if let Some(p) = &self.selected_path { self.path_input = p.display().to_string(); }
                                } else {
                                    self.error = err;
                                }
                            }
                        } else if name.to_lowercase().ends_with(".mix") {
                            if let Some(base) = &self.selected_path {
                                let file_path = base.join(&name);
                                match MixFile::open(&file_path) {
                                    Ok(mix) => { self.mix_preview = Some(mix); self.in_mix_mode = true; self.error = None; self.path_input = file_path.display().to_string(); }
                                    Err(e) => { self.mix_preview = None; self.in_mix_mode = false; self.error = Some(format!("打开 MIX 失败: {}", e)); }
                                }
                            }
                        }
                    }
                }
            });
        });

        // 右侧中央面板：MIX 预览
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
                ui.heading(RichText::new("预览 / 详情 / 操作区").color(Color32::from_rgb(255, 220, 180)));
                ui.add_space(8.0);
                if let Some(mix) = &self.mix_preview {
                    ui.label(RichText::new(format!("MIX: {}", mix.path.display())).strong());
                    ui.label(format!("大小: {}", format_size(mix.file_size)));
                    ui.label(format!("条目数: {}", mix.entries.len()));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("搜索条目(ID十六进制片段)：");
                        let _ = ui.add(TextEdit::singleline(&mut self.mix_search));
                        if ui.button("清空").clicked() { self.mix_search.clear(); }
                        if ui.button("保存副本...").clicked() {
                            // 简易副本保存到同目录 mix.copy.mix
                            let dst = mix.path.with_extension("copy.mix");
                            match mix.save_copy_as(&dst) {
                                Ok(_) => { self.error = None; }
                                Err(e) => { self.error = Some(e); }
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.add(Separator::default());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                        let results = mix.search(&self.mix_search);
                        for e in results {
                            ui.label(format!("id={:08X}  offset={}  size={}", e.id, e.offset, e.size));
                        }
                    });
                } else {
                    ui.label(RichText::new("未选择 MIX 文件").color(Color32::from_gray(170)));
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("mixBrowser")
            .with_inner_size(Vec2::new(1280.0, 800.0))
            .with_min_inner_size(Vec2::new(900.0, 600.0)),
        ..Default::default()
    };

    eframe::run_native(
        "mixBrowser",
        native_options,
        Box::new(|cc| Box::new(AppState::new(cc))),
    )
}
