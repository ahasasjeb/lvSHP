use eframe::egui::{self, Color32, TextureHandle};
use std::io::{Cursor, Read};

use crate::color_match::best_index_rgb;
use crate::palette::Palette;

#[derive(Clone)]
pub struct Frame {
    pub pixels: Vec<u8>,
}

#[derive(Clone)]
pub struct SHP {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<Frame>,
}

impl SHP {
    pub fn new(width: u32, height: u32, frames: usize) -> Self {
        let mut f = Vec::with_capacity(frames);
        for _ in 0..frames { f.push(Frame { pixels: vec![0u8; (width * height) as usize] }); }
        Self { width, height, frames: f }
    }

    pub fn load(bytes: &[u8]) -> Result<Self, String> {
        // 兼容 RA2/YR SHP：
        // Header: u16 zero, u16 width, u16 height, u16 frame_count
        // Per-frame header (24 bytes): x,y,w,h (u16*4), flags(u32), frameColor[4], zero(i32), dataOffset(u32)
        #[derive(Clone, Copy)]
        struct FHeader { x:u16, y:u16, w:u16, h:u16, flags:u32, data_off:u32 }

        fn read_u16(r:&mut Cursor<&[u8]>) -> Result<u16,String>{ let mut b=[0u8;2]; r.read_exact(&mut b).map_err(|e|e.to_string())?; Ok(u16::from_le_bytes(b)) }
        fn read_u32(r:&mut Cursor<&[u8]>) -> Result<u32,String>{ let mut b=[0u8;4]; r.read_exact(&mut b).map_err(|e|e.to_string())?; Ok(u32::from_le_bytes(b)) }
        fn read_i32(r:&mut Cursor<&[u8]>) -> Result<i32,String>{ let mut b=[0u8;4]; r.read_exact(&mut b).map_err(|e|e.to_string())?; Ok(i32::from_le_bytes(b)) }

        if bytes.len() < 8 { return Err("SHP头不足".into()); }
        let mut cur = Cursor::new(bytes);
        let zero = read_u16(&mut cur)?; if zero != 0 { return Err("不是有效的SHP文件".into()); }
        let w = read_u16(&mut cur)? as u32;
        let h = read_u16(&mut cur)? as u32;
        let n = read_u16(&mut cur)? as usize;
        if w == 0 || h == 0 || n == 0 { return Err("无效SHP尺寸/帧数".into()); }

        // 读取帧头
        let mut fhs: Vec<FHeader> = Vec::with_capacity(n);
        for _ in 0..n {
            let x = read_u16(&mut cur)?;
            let y = read_u16(&mut cur)?;
            let ww = read_u16(&mut cur)?;
            let hh = read_u16(&mut cur)?;
            let flags = read_u32(&mut cur)?;
            let mut color_rgba = [0u8;4]; cur.read_exact(&mut color_rgba).map_err(|e|e.to_string())?;
            let _zero2 = read_i32(&mut cur)?; // 忽略
            let data_off = read_u32(&mut cur)?;
            fhs.push(FHeader { x, y, w: ww, h: hh, flags, data_off });
        }

        // 解码帧数据
        let mut frames: Vec<Frame> = Vec::with_capacity(n);
        for fh in fhs.iter().copied() {
            let mut pixels = vec![0u8; (w * h) as usize];
            if fh.data_off == 0 || fh.w == 0 || fh.h == 0 {
                frames.push(Frame { pixels });
                continue;
            }
            if fh.data_off as usize >= bytes.len() { return Err("SHP数据偏移越界".into()); }
            let mut r = Cursor::new(&bytes[fh.data_off as usize..]);
            let is_rle0 = (fh.flags & 3) == 3;
            let is_scan = (fh.flags & 2) == 2 && (fh.flags & 1) == 0;

            if is_rle0 {
                // 每行：u16 长度(含2字节)，随后数据；0后跟零的数量
                let mut y_abs = fh.y as usize;
                for _ in 0..fh.h as usize {
                    let len = read_u16(&mut r)? as usize;
                    let mut remaining = len.saturating_sub(2);
                    let mut x_abs = fh.x as usize;
                    while remaining > 0 {
                        let mut b=[0u8;1]; r.read_exact(&mut b).map_err(|e|e.to_string())?; remaining-=1;
                        let v = b[0];
                        if v != 0 {
                            if x_abs < w as usize && y_abs < h as usize { pixels[y_abs * w as usize + x_abs] = v; }
                            x_abs += 1;
                        } else {
                            let mut c=[0u8;1]; r.read_exact(&mut c).map_err(|e|e.to_string())?; remaining-=1;
                            let zeros = c[0] as usize;
                            x_abs = x_abs.saturating_add(zeros);
                        }
                    }
                    y_abs += 1;
                }
            } else if is_scan {
                // 每行：u16 长度(含2字节)，随后按顺序字节
                let mut y_abs = fh.y as usize;
                for _ in 0..fh.h as usize {
                    let len = read_u16(&mut r)? as usize;
                    let mut remaining = len.saturating_sub(2);
                    let mut x_abs = fh.x as usize;
                    while remaining > 0 {
                        let mut b=[0u8;1]; r.read_exact(&mut b).map_err(|e|e.to_string())?; remaining-=1;
                        let v = b[0];
                        if x_abs < w as usize && y_abs < h as usize { pixels[y_abs * w as usize + x_abs] = v; }
                        x_abs += 1;
                    }
                    y_abs += 1;
                }
            } else {
                // 未压缩：w*h 直接字节块
                let mut y_abs = fh.y as usize;
                for _ in 0..fh.h as usize {
                    let mut row = vec![0u8; fh.w as usize];
                    r.read_exact(&mut row).map_err(|e| e.to_string())?;
                    let mut x_abs = fh.x as usize;
                    for v in row {
                        if x_abs < w as usize && y_abs < h as usize { pixels[y_abs * w as usize + x_abs] = v; }
                        x_abs += 1;
                    }
                    y_abs += 1;
                }
            }

            frames.push(Frame { pixels });
        }

        Ok(Self { width: w, height: h, frames })
    }

    pub fn save(&self) -> Result<Vec<u8>, String> {
        // 保存为 RA2/YR 兼容格式：
        // 8字节头 + N个24字节帧头 + 帧数据（此处使用未压缩块，大小为画布宽*高，每帧）
        if self.frames.is_empty() { return Err("没有帧".into()); }

        let n = self.frames.len();
        let header_size: usize = 8 + 24 * n;

        // 预先为每帧准备原始数据块（未压缩，大小=margin.w*margin.h，这里使用整幅画布）
        let mut frame_blocks: Vec<Vec<u8>> = Vec::with_capacity(n);
        let mut data_offsets: Vec<u32> = vec![0u32; n];

        for fi in 0..n {
            let mut block = Vec::with_capacity((self.width * self.height) as usize);
            block.resize((self.width * self.height) as usize, 0);
            // 复制整幅画布
            for y in 0..self.height as usize {
                for x in 0..self.width as usize {
                    let v = self.frames[fi].pixels[y * self.width as usize + x];
                    block[y * self.width as usize + x] = v;
                }
            }
            frame_blocks.push(block);
        }

        // 计算每帧数据偏移
        let mut cursor: u32 = header_size as u32;
        for (i, blk) in frame_blocks.iter().enumerate() {
            // 如果整帧为空（全0），写偏移就保留0以保持兼容
            let empty = blk.iter().all(|&b| b == 0);
            if empty {
                data_offsets[i] = 0;
            } else {
                data_offsets[i] = cursor;
                cursor = cursor.saturating_add(blk.len() as u32);
            }
        }

        // 写头
        let mut out: Vec<u8> = Vec::with_capacity(cursor as usize);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.width as u16).to_le_bytes());
        out.extend_from_slice(&(self.height as u16).to_le_bytes());
        out.extend_from_slice(&(n as u16).to_le_bytes());

        // 写每帧24字节帧头
        for i in 0..n {
            // x,y,w,h （整幅）
            out.extend_from_slice(&0u16.to_le_bytes()); // x
            out.extend_from_slice(&0u16.to_le_bytes()); // y
            out.extend_from_slice(&(self.width as u16).to_le_bytes());
            out.extend_from_slice(&(self.height as u16).to_le_bytes());
            // flags：未压缩且可透明(或0)。这里用0（Opaque）或1（Transparent）都可，加载分支不依赖flags
            let flags: u32 = 0; // 0=Opaque（简化）
            out.extend_from_slice(&flags.to_le_bytes());
            // frame color (RGB)+0
            out.extend_from_slice(&[0u8, 0, 0, 0]);
            // i32 0
            out.extend_from_slice(&0i32.to_le_bytes());
            // data offset
            out.extend_from_slice(&data_offsets[i].to_le_bytes());
        }

        // 写数据块
        for (i, blk) in frame_blocks.into_iter().enumerate() {
            if data_offsets[i] == 0 { continue; }
            out.extend_from_slice(&blk);
        }

        Ok(out)
    }

    #[allow(dead_code)]
    pub fn set_pixel(&mut self, frame: usize, x: u32, y: u32, index: u8) {
        if frame >= self.frames.len() { return; }
        if x >= self.width || y >= self.height { return; }
        let i = (y * self.width + x) as usize;
        self.frames[frame].pixels[i] = index;
    }

    #[allow(dead_code)]
    pub fn paste_rgba_into_frame(&mut self, frame: usize, rgba: &image::RgbaImage, pal: &Palette) {
        if frame >= self.frames.len() { return; }
        let fw = self.width as i32;
        let fh = self.height as i32;
        let iw = rgba.width() as i32;
        let ih = rgba.height() as i32;
        let offx = (fw - iw) / 2;
        let offy = (fh - ih) / 2;
        for y in 0..ih {
            for x in 0..iw {
                let px = rgba.get_pixel(x as u32, y as u32);
                if px[3] < 8 { continue; }
                let idx = best_index_rgb(Color32::from_rgb(px[0], px[1], px[2]), &pal.colors);
                let tx = x + offx; let ty = y + offy;
                if tx >= 0 && ty >= 0 && tx < fw && ty < fh {
                    let i = (ty as u32 * self.width + tx as u32) as usize;
                    self.frames[frame].pixels[i] = idx;
                }
            }
        }
    }

    pub fn paste_rgba_at(&mut self, frame: usize, rgba: &image::RgbaImage, dest_x: i32, dest_y: i32, pal: &Palette) {
        if frame >= self.frames.len() { return; }
        let fw = self.width as i32;
        let fh = self.height as i32;
        let iw = rgba.width() as i32;
        let ih = rgba.height() as i32;
        for y in 0..ih {
            for x in 0..iw {
                let px = rgba.get_pixel(x as u32, y as u32);
                if px[3] < 8 { continue; }
                let idx = best_index_rgb(Color32::from_rgb(px[0], px[1], px[2]), &pal.colors);
                let tx = x + dest_x; let ty = y + dest_y;
                if tx >= 0 && ty >= 0 && tx < fw && ty < fh {
                    let i = (ty as u32 * self.width + tx as u32) as usize;
                    self.frames[frame].pixels[i] = idx;
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn export_frame_png(&self, frame: usize, pal: &Palette, path: std::path::PathBuf) -> Result<(), String> {
        if frame >= self.frames.len() { return Err("帧索引超界".into()); }
        let mut img = image::RgbaImage::new(self.width, self.height);
        let fr = &self.frames[frame];
        // 约定：调色板索引0为透明
        for y in 0..self.height { for x in 0..self.width {
            let idx = fr.pixels[(y * self.width + x) as usize] as usize;
            let c = pal.colors[idx];
            let a = if idx == 0 { 0u8 } else { 255u8 };
            img.put_pixel(x, y, image::Rgba([c.r(), c.g(), c.b(), a]));
        }}
        image::DynamicImage::ImageRgba8(img).save(path).map_err(|e| e.to_string())
    }

    #[allow(dead_code)]
    pub fn egui_texture(&self, ctx: &egui::Context, frame: usize, pal: &Palette) -> TextureHandle {
        self.egui_texture_with_brightness(ctx, frame, pal, 1.0)
    }

    pub fn egui_texture_with_brightness(
        &self,
        ctx: &egui::Context,
        frame: usize,
        pal: &Palette,
        brightness: f32,
    ) -> TextureHandle {
        // 安全保护：避免异常尺寸导致巨大内存分配
        let pixels_u64 = (self.width as u64) * (self.height as u64);
        if pixels_u64 == 0 || pixels_u64 > 64_000_000 { // 上限约 64M 像素（~256MB RGBA）
            let img = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0u8, 0, 0, 255]);
            return ctx.load_texture("frame_tex_err", img, egui::TextureOptions::NEAREST);
        }
        let mut rgba = Vec::with_capacity((pixels_u64 * 4) as usize);
        let fr = if frame < self.frames.len() { &self.frames[frame] } else { &self.frames[0] };
        let b = brightness.max(0.2).min(3.0);
        let total = (self.width * self.height) as usize;
        for i in 0..total {
            let idx = fr.pixels[i] as usize;
            let c = pal.colors[idx];
            let r = ((c.r() as f32) * b).round().min(255.0) as u8;
            let g = ((c.g() as f32) * b).round().min(255.0) as u8;
            let bl = ((c.b() as f32) * b).round().min(255.0) as u8;
            let a = if idx == 0 { 0u8 } else { 255u8 }; // 预览中索引0透明
            rgba.push(r); rgba.push(g); rgba.push(bl); rgba.push(a);
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([self.width as usize, self.height as usize], &rgba);
        ctx.load_texture("frame_tex", img, egui::TextureOptions::NEAREST)
    }
}


