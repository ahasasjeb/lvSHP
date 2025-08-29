use std::time::Instant;

use eframe::egui::{self, Color32, Context, Key, Modifiers, RichText, Sense};
use rfd::FileDialog;

use crate::image_io;
use crate::palette::Palette;

use crate::shp::SHP;

// 内置字体：构建时打包 wqy-microhei.ttc
const EMBED_WQY_MICROHEI: &[u8] = include_bytes!("../wqy-microhei.ttc");

pub struct MixApp {
    pub palette: Palette,
    pub shp: Option<SHP>,
    // UI state
    pub brush_index: u8,
    pub tool: Tool,
    pub scale: f32,
    pub brush_size: u32,
    // 绘图状态
    pub drawing: bool,
    pub draw_start: Option<egui::Pos2>,
    pub draw_end: Option<egui::Pos2>,
    pub fill_mode: bool,
    pub preview: PreviewState,
    pub status: String,
    // New SHP dialog
    pub show_new_dialog: bool,
    pub new_w: u32,
    pub new_h: u32,
    pub new_frames: usize,
    // built-in palettes & display
    pub current_pal_name: String,
    pub brightness: f32,
    // import gizmo
    pub import_img: Option<image::RgbaImage>,
    pub import_pos: egui::Pos2,
    pub import_scale: f32,
    pub import_angle_deg: f32,
    pub import_armed: bool,
    // grouped palettes by folder
    pub grouped_pals: Vec<(String, Vec<(String, Palette)>)>,
    pub dirty: bool,
    pub show_exit_confirm: bool,
    // 撤销/重做
    pub undo_stack: Vec<Vec<u8>>, // 当前帧历史
    pub redo_stack: Vec<Vec<u8>>, // 当前帧重做
    pub max_undo_steps: usize,
    // 撤销历史所属的帧锚点：当当前帧变化时清空历史，避免跨帧污染
    pub undo_frame_anchor: Option<usize>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Eraser,
    Line,
    Rectangle,
    Circle,
    Fill,
}

pub struct PreviewState {
    pub playing: bool,
    pub current_frame: usize,
    pub ms_per_frame: u64,
    pub last_tick: Instant,
    pub accumulator_ms: u64,
}

impl PreviewState {
    pub fn new() -> Self {
        Self {
            playing: false,
            current_frame: 0,
            ms_per_frame: 150,
            last_tick: Instant::now(),
            accumulator_ms: 0,
        }
    }

    pub fn tick(&mut self, frame_count: usize) -> Option<usize> {
        if !self.playing || frame_count == 0 { return None; }
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        self.accumulator_ms = self.accumulator_ms.saturating_add(dt.as_millis() as u64);
        let mut advanced = 0usize;
        while self.accumulator_ms >= self.ms_per_frame {
            self.accumulator_ms -= self.ms_per_frame;
            self.current_frame = (self.current_frame + 1) % frame_count;
            advanced += 1;
        }
        if advanced > 0 { Some(self.current_frame) } else { None }
    }
}

impl MixApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_theme(&cc.egui_ctx);
        // load embedded or filesystem palettes
        let (grouped, flat): (Vec<(String, Vec<(String, Palette)>)>, Vec<(String, Palette)>) = load_embedded_palettes();
        let default_pal = flat.first().map(|(_, p)| p.clone()).unwrap_or_else(Palette::default_grayscale);

        Self {
            palette: default_pal,
            shp: None,
            brush_index: 1,
            tool: Tool::Pencil,
            scale: 4.0,
            brush_size: 1,
            drawing: false,
            draw_start: None,
            draw_end: None,
            fill_mode: false,
            preview: PreviewState::new(),
            status: String::new(),
            show_new_dialog: false,
            new_w: 256,
            new_h: 256,
            new_frames: 64,

            current_pal_name: "Grayscale".into(),
            brightness: 1.2,
            import_img: None,
            import_pos: egui::pos2(0.0, 0.0),
            import_scale: 1.0,
            import_angle_deg: 0.0,
            import_armed: false,
            grouped_pals: grouped,
            dirty: false,
            show_exit_confirm: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_steps: 100,
            undo_frame_anchor: None,
        }
    }

    // 撤销/重做
    #[allow(dead_code)]
    fn save_undo_state_for_frame(&mut self, frame_idx: usize) {
        if let Some(shp) = &self.shp {
            let data = shp.frames[frame_idx].pixels.clone();
            self.undo_stack.push(data);
            if self.undo_stack.len() > self.max_undo_steps { self.undo_stack.remove(0); }
            self.redo_stack.clear();
        }
    }

    fn undo(&mut self) {
        if let Some(shp) = &mut self.shp {
            let fi = self.preview.current_frame.min(shp.frames.len().saturating_sub(1));
            // 帧锚点校验：若已切换帧，清空历史避免跨帧污染
            if self.undo_frame_anchor.map_or(false, |a| a != fi) {
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.undo_frame_anchor = Some(fi);
                self.status = "已切换帧，撤销历史已清空".to_owned();
                return;
            }
            if let Some(prev) = self.undo_stack.pop() {
                let cur = std::mem::replace(&mut shp.frames[fi].pixels, prev);
                self.redo_stack.push(cur);
                self.dirty = true;
                self.status = "已撤销".to_owned();
            }
        }
    }

    fn redo(&mut self) {
        if let Some(shp) = &mut self.shp {
            let fi = self.preview.current_frame.min(shp.frames.len().saturating_sub(1));
            // 帧锚点校验：若已切换帧，清空历史避免跨帧污染
            if self.undo_frame_anchor.map_or(false, |a| a != fi) {
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.undo_frame_anchor = Some(fi);
                self.status = "已切换帧，重做历史已清空".to_owned();
                return;
            }
            if let Some(next_) = self.redo_stack.pop() {
                let cur = std::mem::replace(&mut shp.frames[fi].pixels, next_);
                self.undo_stack.push(cur);
                self.dirty = true;
                self.status = "已重做".to_owned();
            }
        }
    }

    // ===== 画图算法（在不修改SHP的前提下）=====
    fn frame_set_pixel(shp: &mut SHP, frame_idx: usize, x: i32, y: i32, color: u8) {
        if frame_idx >= shp.frames.len() { return; }
        if x < 0 || y < 0 { return; }
        let (x, y) = (x as u32, y as u32);
        if x >= shp.width || y >= shp.height { return; }
        let i = (y * shp.width + x) as usize;
        shp.frames[frame_idx].pixels[i] = color;
    }

    fn frame_get_pixel(shp: &SHP, frame_idx: usize, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 { return 0; }
        let (x, y) = (x as u32, y as u32);
        if frame_idx >= shp.frames.len() || x >= shp.width || y >= shp.height { return 0; }
        let i = (y * shp.width + x) as usize;
        shp.frames[frame_idx].pixels[i]
    }

    fn draw_line_on_frame(shp: &mut SHP, fi: usize, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: u8) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            Self::frame_set_pixel(shp, fi, x0, y0, color);
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }

    fn draw_rect_on_frame(shp: &mut SHP, fi: usize, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (lx, rx) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (ty, by) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        Self::draw_line_on_frame(shp, fi, lx, ty, rx, ty, color);
        Self::draw_line_on_frame(shp, fi, lx, by, rx, by, color);
        Self::draw_line_on_frame(shp, fi, lx, ty, lx, by, color);
        Self::draw_line_on_frame(shp, fi, rx, ty, rx, by, color);
    }

    fn fill_rect_on_frame(shp: &mut SHP, fi: usize, x0: i32, y0: i32, x1: i32, y1: i32, color: u8) {
        let (lx, rx) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (ty, by) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in ty..=by { for x in lx..=rx { Self::frame_set_pixel(shp, fi, x, y, color); } }
    }

    fn draw_circle_on_frame(shp: &mut SHP, fi: usize, cx: i32, cy: i32, radius: i32, color: u8) {
        if radius <= 0 { return; }
        let mut x = radius; let mut y = 0; let mut err = 1 - x;
        while x >= y {
            let pts = [
                (cx + x, cy + y), (cx + y, cy + x), (cx - y, cy + x), (cx - x, cy + y),
                (cx - x, cy - y), (cx - y, cy - x), (cx + y, cy - x), (cx + x, cy - y),
            ];
            for (px, py) in pts { Self::frame_set_pixel(shp, fi, px, py, color); }
            y += 1;
            if err < 0 { err += 2*y + 1; } else { x -= 1; err += 2*(y - x) + 1; }
        }
    }

    fn fill_circle_on_frame(shp: &mut SHP, fi: usize, cx: i32, cy: i32, radius: i32, color: u8) {
        if radius <= 0 { return; }
        let r2 = (radius as i64) * (radius as i64);
        let min_y = cy - radius; let max_y = cy + radius;
        for y in min_y..=max_y {
            let dy = y as i64 - cy as i64; let xr2 = r2 - dy*dy; if xr2 < 0 { continue; }
            let dx = (xr2 as f64).sqrt() as i32; let lx = cx - dx; let rx = cx + dx;
            for x in lx..=rx { Self::frame_set_pixel(shp, fi, x, y, color); }
        }
    }

    // 用于铅笔/橡皮的“圆形笔刷”着色：根据大小在中心处绘制实心圆
    fn stamp_disc_on_frame(shp: &mut SHP, fi: usize, cx: i32, cy: i32, size: u32, color: u8) {
        if size <= 1 { Self::frame_set_pixel(shp, fi, cx, cy, color); return; }
        // 半径：与常见像素画工具一致，取 size 的半径向下取整
        let radius = ((size as i32) - 1) / 2;
        Self::fill_circle_on_frame(shp, fi, cx, cy, radius.max(1), color);
    }

    fn flood_fill_on_frame(shp: &mut SHP, fi: usize, x: i32, y: i32, new_color: u8) {
        if fi >= shp.frames.len() { return; }
        let w = shp.width as i32; let h = shp.height as i32;
        let target = Self::frame_get_pixel(shp, fi, x, y);
        if target == new_color { return; }
        let mut stack = vec![(x, y)];
        while let Some((px, py)) = stack.pop() {
            if px < 0 || py < 0 || px >= w || py >= h { continue; }
            if Self::frame_get_pixel(shp, fi, px, py) != target { continue; }
            Self::frame_set_pixel(shp, fi, px, py, new_color);
            stack.push((px-1, py)); stack.push((px+1, py));
            stack.push((px, py-1)); stack.push((px, py+1));
        }
    }

    

    pub fn ui_menu(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.menu_button("文件", |ui| {
            if ui.button("新建 SHP...").clicked() { ui.close_menu(); self.show_new_dialog = true; }
            if ui.button("打开 SHP...").clicked() {
                ui.close_menu();
                self.action_open_shp();
            }
            if ui.button("保存 SHP...").clicked() {
                ui.close_menu();
                self.action_save_shp();
            }
            ui.separator();
            ui.menu_button("选择内置PAL", |ui| {
                for (group, items) in &self.grouped_pals {
                    ui.menu_button(group, |ui| {
                        for (name, pal) in items {
                            if ui.selectable_label(self.current_pal_name==*name, name).clicked() {
                                self.palette = pal.clone();
                                self.current_pal_name = name.clone();
                                self.dirty = true; // 切换调色板会影响显示，标记为需要保存
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
            if ui.button("打开 PAL...").clicked() {
                ui.close_menu();
                self.action_open_pal();
            }
            if ui.button("保存 PAL...").clicked() {
                ui.close_menu();
                self.action_save_pal();
            }
            ui.separator();
            if ui.button("导入图片为帧 (PNG/JPG/GIF/APNG)...").clicked() {
                ui.close_menu();
                self.action_import_image(ctx);
            }
            if ui.button("导出当前帧为 PNG...").clicked() {
                ui.close_menu();
                self.action_export_png();
            }
        });

        ui.menu_button("预览", |ui| {
            if ui.button(if self.preview.playing { "暂停" } else { "播放" }).clicked() {
                self.preview.playing = !self.preview.playing;
                self.preview.last_tick = Instant::now();
                ui.close_menu();
            }
            ui.add(egui::Slider::new(&mut self.preview.ms_per_frame, 30..=500).text("间隔ms"));
        });

        // 顶部不再放工具菜单，遵循“左侧工具箱”设计

        ui.separator();
        ui.label(RichText::new(&self.status).color(Color32::LIGHT_GRAY));
    }

    fn action_new_shp(&mut self) {
        // 简化：固定弹窗交互改为默认值；后续补对话框
        let width = 128u32;
        let height = 128u32;
        let frames = 8usize;
        self.shp = Some(SHP::new(width, height, frames));
        self.preview.current_frame = 0;
        self.status = format!("已新建 SHP: {}x{}, 帧数 {}", width, height, frames);
        // 新建后复位编辑状态，避免历史遗留
        self.dirty = false; // 新建文件，清除dirty标记
        self.import_img = None;
        self.import_armed = false;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.undo_frame_anchor = Some(0);
        self.preview.playing = false;
    }

    fn action_open_shp(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("SHP", &["shp"]).pick_file() {
            match std::fs::read(&path) {
                Ok(bytes) => match SHP::load(&bytes) {
                    Ok(shp) => { 
                        self.shp = Some(shp); 
                        self.status = format!("已加载 SHP: {}", path.display()); 
                        // 打开后复位编辑状态，避免历史遗留
                        self.preview.current_frame = 0;
                        self.dirty = false; // 打开新文件，清除dirty标记
                        self.import_img = None;
                        self.import_armed = false;
                        self.undo_stack.clear();
                        self.redo_stack.clear();
                        self.undo_frame_anchor = Some(0);
                        self.preview.playing = false;
                    }
                    Err(e) => { self.status = format!("加载SHP失败: {}", e); }
                },
                Err(e) => { self.status = format!("读取文件失败: {}", e); }
            }
        }
    }

    fn action_save_shp(&mut self) {
        if let Some(shp) = &self.shp {
            if let Some(path) = FileDialog::new().set_file_name("output.shp").save_file() {
                match shp.save() {
                    Ok(bytes) => {
                        if let Err(e) = std::fs::write(&path, bytes) { 
                            self.status = format!("保存失败: {}", e); 
                        } else { 
                            self.status = format!("已保存: {}", path.display()); 
                            self.dirty = false; // 保存成功后清除dirty标记
                        }
                    }
                    Err(e) => { self.status = format!("导出SHP失败: {}", e); }
                }
            }
        } else {
            self.status = "当前没有SHP".into();
        }
    }

    fn action_open_pal(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("PAL", &["pal"]).pick_file() {
            match std::fs::read(&path) {
                Ok(bytes) => match Palette::from_bytes(&bytes) {
                    Ok(p) => { 
                        self.palette = p; 
                        self.status = format!("已加载 PAL: {}", path.display()); 
                        self.dirty = true; // 切换调色板会影响显示，标记为需要保存
                    }
                    Err(e) => { self.status = format!("加载PAL失败: {}", e); }
                },
                Err(e) => { self.status = format!("读取文件失败: {}", e); }
            }
        }
    }

    fn action_save_pal(&mut self) {
        if let Some(path) = FileDialog::new().set_file_name("palette.pal").save_file() {
            let bytes = self.palette.to_bytes();
            if let Err(e) = std::fs::write(&path, bytes) {
                self.status = format!("保存PAL失败: {}", e);
            } else {
                self.status = format!("已保存 PAL: {}", path.display());
            }
        }
    }

    fn action_import_image(&mut self, _ctx: &Context) {
        if self.shp.is_none() { self.status = "请先新建或打开SHP".into(); return; }
        if let Some(path) = FileDialog::new().add_filter("图片", &["png","jpg","jpeg","gif","apng"]).pick_file() {
            match image_io::load_rgba_frames(&path) {
                Ok(frames) => {
                    // 取首帧作为导入源；进入Gizmo编辑态
                    if let Some(rgba) = frames.get(0) {
                        self.import_img = Some(rgba.clone());
                        self.import_pos = egui::pos2(0.0, 0.0);
                        self.import_scale = 1.0;
                        self.import_angle_deg = 0.0;
                        self.status = format!("已载入 {}，请在画布上拖动/缩放/固定。", path.display());
                        self.import_armed = false; // 避免首次导入立即被外部点击固定
                    }
                }
                Err(e) => { self.status = format!("导入失败: {}", e); }
            }
        }
    }

    fn action_export_png(&mut self) {
        if let Some(shp) = &self.shp {
            if let Some(path) = FileDialog::new().set_file_name("frame.png").save_file() {
                let idx = self.preview.current_frame.min(shp.frames.len().saturating_sub(1));
                match shp.export_frame_png(idx, &self.palette, path.clone()) {
                    Ok(()) => { self.status = format!("已导出: {}", path.display()); }
                    Err(e) => { self.status = format!("导出失败: {}", e); }
                }
            }
        } else {
            self.status = "当前没有SHP".into();
        }
    }
}

fn load_embedded_palettes() -> (Vec<(String, Vec<(String, Palette)>)>, Vec<(String, Palette)>) {
    // 仅从内置资源读取，避免外部目录递归导致的潜在内存膨胀/循环引用
    let grouped = crate::palette::EmbeddedPalettes::grouped_by_folder();
    // 拍平为 (name, palette) 列表
    let mut flat: Vec<(String, Palette)> = Vec::new();
    for (_, items) in &grouped { for (n, p) in items { flat.push((n.clone(), p.clone())); } }
    if flat.is_empty() { flat.push(("Grayscale".into(), Palette::default_grayscale())); }
    (grouped, flat)
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 搜索字体：项目根目录或可执行文件旁
    let mut data_opt: Option<Vec<u8>> = None;
    let candidates = [
        std::path::PathBuf::from("wqy-microhei.ttc"),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("wqy-microhei.ttc"))).unwrap_or_default(),
    ];
    for p in candidates.iter() {
        if p.as_os_str().is_empty() { continue; }
        if let Ok(bytes) = std::fs::read(p) { data_opt = Some(bytes); break; }
    }

    if data_opt.is_none() {
        // 使用内置字体
        data_opt = Some(EMBED_WQY_MICROHEI.to_vec());
    }

    if let Some(bytes) = data_opt {
        fonts.font_data.insert("wqy".to_owned(), egui::FontData::from_owned(bytes));
        // 将中文字体置于优先位置
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "wqy".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "wqy".to_owned());
        ctx.set_fonts(fonts);
    }
}

fn setup_theme(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
}

impl eframe::App for MixApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 播放时主动驱动重绘，避免无输入时不刷新导致不播放
        if self.preview.playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(10));
        }
        // 顶部菜单栏
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| { self.ui_menu(ui, ctx); });
        });

        // 左侧：工具与调色板（Windows画图风格）
        egui::SidePanel::left("left").resizable(true).default_width(280.0).show(ctx, |ui| {
            // 撤销/重做快捷按钮
            let can_undo = !self.undo_stack.is_empty();
            let can_redo = !self.redo_stack.is_empty();
            ui.horizontal(|ui| {
                if ui.add_enabled(can_undo, egui::Button::new("撤销 (Ctrl+Z)")).clicked() { self.undo(); }
                if ui.add_enabled(can_redo, egui::Button::new("重做 (Ctrl+Y)")).clicked() { self.redo(); }
            });
            ui.separator();
            ui.heading("工具");
            egui::Grid::new("tools_grid").num_columns(2).spacing([6.0,6.0]).show(ui, |ui| {
                if ui.selectable_label(self.tool==Tool::Pencil, "✏️ 铅笔").clicked(){ self.tool=Tool::Pencil; }
                if ui.selectable_label(self.tool==Tool::Eraser, "🧽 橡皮").clicked(){ self.tool=Tool::Eraser; }
                ui.end_row();
                if ui.selectable_label(self.tool==Tool::Fill, "🪣 填充").clicked(){ self.tool=Tool::Fill; }
                if ui.selectable_label(self.tool==Tool::Line, "📏 直线").clicked(){ self.tool=Tool::Line; }
                ui.end_row();
                if ui.selectable_label(self.tool==Tool::Rectangle, "⬛ 矩形").clicked(){ self.tool=Tool::Rectangle; }
                if ui.selectable_label(self.tool==Tool::Circle, "⚪ 圆").clicked(){ self.tool=Tool::Circle; }
                ui.end_row();
            });
            ui.separator();
            ui.label("画笔大小");
            ui.add(egui::Slider::new(&mut self.brush_size, 1..=20).text("px"));
            if matches!(self.tool, Tool::Rectangle | Tool::Circle) { ui.checkbox(&mut self.fill_mode, "填充形状"); }
            ui.separator();
            ui.heading("调色板");
            let mut chosen = self.brush_index;
            let desired_columns = 16usize;
            egui::Grid::new("pal-grid").spacing([2.0, 2.0]).show(ui, |ui| {
                for row in 0..16 {
                    for col in 0..16 {
                        let idx = (row * desired_columns + col) as u8;
                        let color = self.palette.colors[idx as usize];
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::click());
                        ui.painter().rect_filled(rect, 0.0, color);
                        if response.clicked() { chosen = idx; }
                    }
                    ui.end_row();
                }
            });
            self.brush_index = chosen;
            let c = self.palette.colors[self.brush_index as usize];
            ui.horizontal(|ui| {
                ui.label(format!("索引 {}", self.brush_index));
                let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 14.0), Sense::hover());
                ui.painter().rect_filled(rect, 2.0, c);
            });
            ui.add(egui::Slider::new(&mut self.brightness, 0.5..=3.0).text("预览亮度"));
        });

        // 底部：帧与预览控制
        egui::TopBottomPanel::bottom("bottom").default_height(120.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("缩放");
                ui.add(egui::Slider::new(&mut self.scale, 1.0..=12.0));
                ui.separator();
                ui.checkbox(&mut self.preview.playing, "播放");
                ui.add(egui::Slider::new(&mut self.preview.ms_per_frame, 30..=500).text("间隔ms"));
            });

            if let Some(shp) = &mut self.shp {
                let count = shp.frames.len();
                let _ = self.preview.tick(count);
                ui.separator();
                ui.horizontal(|ui| {
                    let prev_disabled = self.preview.current_frame == 0;
                    let next_disabled = self.preview.current_frame + 1 >= count;
                    if ui.add_enabled(!prev_disabled, egui::Button::new("← 上一帧")).clicked() {
                        if self.preview.current_frame > 0 { self.preview.current_frame -= 1; }
                    }
                    let mut frame_val = self.preview.current_frame as u32;
                    ui.add(egui::Slider::new(&mut frame_val, 0..=count.saturating_sub(1) as u32).text("帧"));
                    self.preview.current_frame = frame_val as usize;
                    if ui.add_enabled(!next_disabled, egui::Button::new("下一帧 →")).clicked() {
                        if self.preview.current_frame + 1 < count { self.preview.current_frame += 1; }
                    }
                    ui.label(format!("/ 共 {} 帧", count));
                });
                // 帧切换锚点：一旦当前帧不同于撤销历史所属帧，清空撤销/重做，避免跨帧污染
                let cur = self.preview.current_frame.min(count.saturating_sub(1));
                match self.undo_frame_anchor {
                    None => self.undo_frame_anchor = Some(cur),
                    Some(anchor) if anchor != cur => {
                        self.undo_stack.clear();
                        self.redo_stack.clear();
                        self.undo_frame_anchor = Some(cur);
                    }
                    _ => {}
                }
            }
        });

        // 中央：画布
        egui::CentralPanel::default().show(ctx, |ui| {
            let mut pending_undo: Option<Vec<u8>> = None;
            if let Some(shp) = &mut self.shp {
                let frame_idx = self.preview.current_frame.min(shp.frames.len().saturating_sub(1));
                let tex = shp.egui_texture_with_brightness(ui.ctx(), frame_idx, &self.palette, self.brightness);
                let size = tex.size_vec2() * self.scale;
                let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                // 画棋盘背景，便于透明像素可见
                {
                    let sq = 8.0_f32.max(self.scale); // 方格尺寸随缩放变化
                    let mut y = rect.top();
                    let dark = egui::Color32::from_gray(60);
                    let light = egui::Color32::from_gray(90);
                    let mut row = 0;
                    while y < rect.bottom() {
                        let mut x = rect.left();
                        let row_offset = row % 2;
                        let mut col = 0;
                        while x < rect.right() {
                            let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(sq, sq));
                            let c = if (col + row_offset) % 2 == 0 { light } else { dark };
                            ui.painter().rect_filled(r.intersect(rect), 0.0, c);
                            x += sq; col += 1;
                        }
                        y += sq; row += 1;
                    }
                }
                ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);

                // 绘制/取色逻辑 + 撤销记录
                // 更稳健的输入判定：鼠标在画布内即处理
                let pointer_pos_opt = ui.input(|i| i.pointer.interact_pos());
                let pointer_down = ui.input(|i| i.pointer.primary_down());
                if let Some(pp) = pointer_pos_opt { if rect.contains(pp) {
                    let pos = response.interact_pointer_pos().unwrap_or(rect.min);
                    let local = (pos - rect.min) / self.scale;
                    let x = local.x.floor() as i32; let y = local.y.floor() as i32;

                    if response.clicked() || (pointer_down && !self.drawing) {
                        // 无论何种工具，都在操作开始时记录一次撤销点
                        pending_undo = Some(shp.frames[frame_idx].pixels.clone());
                        self.drawing = true;
                        self.draw_start = Some(egui::pos2(x as f32, y as f32));
                        self.draw_end = Some(egui::pos2(x as f32, y as f32));
                        match self.tool {
                            Tool::Pencil => { Self::stamp_disc_on_frame(shp, frame_idx, x, y, self.brush_size, self.brush_index); self.dirty=true; },
                            Tool::Eraser => { Self::stamp_disc_on_frame(shp, frame_idx, x, y, self.brush_size, 0); self.dirty=true; },
                            // 填充为一次性操作：立即完成并结束drawing
                            Tool::Fill => { Self::flood_fill_on_frame(shp, frame_idx, x, y, self.brush_index); self.dirty=true; self.drawing=false; },
                            _ => {}
                        }
                    }
                    if response.dragged() || (pointer_down && self.drawing) {
                        self.draw_end = Some(egui::pos2(x as f32, y as f32));
                        match self.tool {
                            Tool::Pencil => { Self::stamp_disc_on_frame(shp, frame_idx, x, y, self.brush_size, self.brush_index); self.dirty=true; },
                            Tool::Eraser => { Self::stamp_disc_on_frame(shp, frame_idx, x, y, self.brush_size, 0); self.dirty=true; },
                            _ => {}
                        }
                    }
                    if (!pointer_down) && self.drawing {
                        self.drawing = false;
                        if let (Some(s), Some(e)) = (self.draw_start, self.draw_end) {
                            let x0 = s.x as i32; let y0 = s.y as i32; let x1 = e.x as i32; let y1 = e.y as i32;
                            match self.tool {
                                Tool::Line => { Self::draw_line_on_frame(shp, frame_idx, x0, y0, x1, y1, self.brush_index); self.dirty=true; },
                                Tool::Rectangle => { if self.fill_mode { Self::fill_rect_on_frame(shp, frame_idx, x0, y0, x1, y1, self.brush_index); } else { Self::draw_rect_on_frame(shp, frame_idx, x0, y0, x1, y1, self.brush_index); } self.dirty=true; },
                                Tool::Circle => { let r = (((x1-x0)*(x1-x0) + (y1-y0)*(y1-y0)) as f32).sqrt() as i32; if self.fill_mode { Self::fill_circle_on_frame(shp, frame_idx, x0, y0, r, self.brush_index); } else { Self::draw_circle_on_frame(shp, frame_idx, x0, y0, r, self.brush_index); } self.dirty=true; },
                                _ => {}
                            }
                        }
                        self.draw_start=None; self.draw_end=None;
                    }
                }}

                // 绘制形状预览
                if self.drawing { if let (Some(s), Some(e)) = (self.draw_start, self.draw_end) {
                    let start = rect.min + egui::vec2(s.x * self.scale, s.y * self.scale);
                    let end   = rect.min + egui::vec2(e.x * self.scale, e.y * self.scale);
                    match self.tool { 
                        Tool::Line => { let _ = ui.painter().line_segment([start,end], egui::Stroke::new(1.0, egui::Color32::WHITE)); }
                        Tool::Rectangle => { let r = egui::Rect::from_two_pos(start,end); let _ = ui.painter().rect_stroke(r,0.0, egui::Stroke::new(1.0, egui::Color32::WHITE)); }
                        Tool::Circle => { let r = start.distance(end); let _ = ui.painter().circle_stroke(start, r, egui::Stroke::new(1.0, egui::Color32::WHITE)); }
                        _ => {}
                    }
                }}

                // 导入图片Gizmo（拖动/缩放，点击外部固定）
                if let Some(img) = &self.import_img {
                    let img_w = img.width();
                    let img_h = img.height();
                    let gizmo_size = egui::vec2((img_w as f32)*self.scale*self.import_scale, (img_h as f32)*self.scale*self.import_scale);
                    let gizmo_rect = egui::Rect::from_min_size(rect.min + (self.import_pos.to_vec2()*self.scale), gizmo_size);
                    ui.painter().rect_stroke(gizmo_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::YELLOW));
                    ui.painter().rect_filled(gizmo_rect, 0.0, egui::Color32::from_rgba_unmultiplied(255,255,255,20));
                    let gizmo_resp = ui.interact(gizmo_rect, ui.id().with("import_gizmo"), Sense::click_and_drag());
                    if gizmo_resp.dragged() { let d = gizmo_resp.drag_delta()/self.scale; self.import_pos.x += d.x; self.import_pos.y += d.y; }

                    let mut should_fix = false;
                    let mut should_cancel = false;
                    egui::Area::new("import_toolbar".into()).fixed_pos(rect.min + egui::vec2(8.0, 8.0)).show(ctx, |ui| {
                        egui::Frame::none().fill(egui::Color32::from_rgba_unmultiplied(0,0,0,128)).show(ui, |ui| {
                            ui.label("导入图变换");
                            ui.add(egui::Slider::new(&mut self.import_scale, 0.1..=8.0).text("缩放"));
                            if ui.button("固定到帧").clicked() { should_fix = true; }
                            if ui.button("取消").clicked() { should_cancel = true; }
                        });
                    });

                    // 仅当已“武装”后才允许通过点击gizmo外部来固定
                    // 初次导入后，等到鼠标没有按下的一个刷新帧后，才设置为武装状态
                    ctx.input(|i| {
                        if !self.import_armed {
                            if !i.pointer.any_down() { self.import_armed = true; }
                        }
                    });

                    let mut clicked_outside_pressed = false;
                    ctx.input(|i| {
                        if i.pointer.primary_pressed() {
                            if let Some(pos) = i.pointer.interact_pos() {
                                if !gizmo_rect.contains(pos) { clicked_outside_pressed = true; }
                            }
                        }
                    });
                    if self.import_armed && clicked_outside_pressed { should_fix = true; }

                    if should_fix {
                        // 缩放尺寸安全上限，防止误操作导致超大分配
                        let mut sw = (img_w as f32 * self.import_scale).round().max(1.0) as u32;
                        let mut sh = (img_h as f32 * self.import_scale).round().max(1.0) as u32;
                        let max_side = 4096u32;
                        if sw > max_side { let k = max_side as f32 / sw as f32; sw = max_side; sh = (sh as f32 * k).round().max(1.0) as u32; }
                        if sh > max_side { let k = max_side as f32 / sh as f32; sh = max_side; sw = (sw as f32 * k).round().max(1.0) as u32; }
                        let resized = image::imageops::resize(img, sw, sh, image::imageops::Nearest);
                        let dest_x = self.import_pos.x.round() as i32; let dest_y = self.import_pos.y.round() as i32;
                        shp.paste_rgba_at(frame_idx, &resized, dest_x, dest_y, &self.palette);
                        self.dirty = true;
                        self.import_img = None;
                    }
                    if should_cancel { self.import_img = None; }
                    // 一帧展示后才允许外部点击固定
                    self.import_armed = true;
                }
            } else { ui.centered_and_justified(|ui| { ui.label("新建或打开一个 SHP 开始绘制"); }); }

            // 在释放对shp的可变借用后，推入撤销栈
            if let Some(data) = pending_undo {
                self.undo_stack.push(data);
                if self.undo_stack.len() > self.max_undo_steps { self.undo_stack.remove(0); }
                self.redo_stack.clear();
                // 记录历史所属的当前帧
                if let Some(shp) = &self.shp {
                    let fi = self.preview.current_frame.min(shp.frames.len().saturating_sub(1));
                    self.undo_frame_anchor = Some(fi);
                }
            }
        });

        // 快捷键
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::N)) { self.action_new_shp(); }
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::O)) { self.action_open_shp(); }
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::S)) { self.action_save_shp(); }
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::Z)) { self.undo(); }
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::Y)) { self.redo(); }
        if ctx.input(|i| i.key_pressed(Key::ArrowLeft)) {
            if let Some(shp) = &self.shp { if self.preview.current_frame > 0 && shp.frames.len() > 0 { self.preview.current_frame -= 1; } }
        }
        if ctx.input(|i| i.key_pressed(Key::ArrowRight)) {
            if let Some(shp) = &self.shp { if self.preview.current_frame + 1 < shp.frames.len() { self.preview.current_frame += 1; } }
        }

        // 退出保护：拦截窗口关闭请求
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_exit_confirm = true;
        }
        if self.show_exit_confirm {
            egui::Window::new("⚠️ 确认退出")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(egui::Frame::window(&ctx.style()).fill(egui::Color32::from_rgba_unmultiplied(30,30,30,240)))
                .show(ctx, |ui| {
                    ui.heading(if self.dirty { "有未保存的更改" } else { "退出程序" });
                    ui.separator();
                    ui.label("建议先保存再退出，避免丢失编辑。");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new("💾 保存并退出").fill(egui::Color32::from_rgb(0,120,0))).clicked() {
                            if let Some(shp) = &self.shp {
                                if let Some(path) = FileDialog::new().set_file_name("output.shp").save_file() {
                                    if let Ok(bytes) = shp.save() {
                                        let _ = std::fs::write(&path, bytes);
                                        self.dirty = false;
                                    }
                                }
                            }
                            self.show_exit_confirm = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.add(egui::Button::new("🗙 不保存退出").fill(egui::Color32::from_rgb(120,0,0))).clicked() {
                            self.show_exit_confirm = false;
                            self.dirty = false;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.add(egui::Button::new("取消").fill(egui::Color32::DARK_GRAY)).clicked() {
                            self.show_exit_confirm = false;
                        }
                    });
                });
        }

        // 键盘快捷键退出确认
        if ctx.input(|i| i.modifiers == Modifiers::CTRL && i.key_pressed(Key::Q)) {
            if self.dirty {
                self.show_exit_confirm = true;
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // 新建SHP大弹窗
        if self.show_new_dialog {
            egui::Window::new("新建 SHP")
                .collapsible(false)
                .resizable(false)
                .fixed_size(egui::vec2(420.0, 240.0))
                .show(ctx, |ui| {
                    ui.label("请输入尺寸与帧数：");
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("宽"); ui.add(egui::DragValue::new(&mut self.new_w).clamp_range(1..=4096));
                        ui.label("高"); ui.add(egui::DragValue::new(&mut self.new_h).clamp_range(1..=4096));
                        ui.label("帧数"); ui.add(egui::DragValue::new(&mut self.new_frames).clamp_range(1..=20000));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("确定").clicked() {
                            self.shp = Some(SHP::new(self.new_w, self.new_h, self.new_frames));
                            self.preview.current_frame = 0;
                            self.status = format!("已新建 SHP: {}x{}, 帧数 {}", self.new_w, self.new_h, self.new_frames);
                            self.show_new_dialog = false;
                            self.dirty = false; // 新建文件，清除dirty标记
                        }
                        if ui.button("取消").clicked() { self.show_new_dialog = false; }
                    });
                });
        }
    }
}


