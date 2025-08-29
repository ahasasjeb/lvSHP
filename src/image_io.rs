use std::path::Path;

use image;

/// 从磁盘加载图片为 RGBA8 帧列表
/// - png/jpg/jpeg：返回单帧
/// - gif：返回所有帧（已转换为 RGBA），若无帧报错
/// - apng：为简化，仅取首帧
pub fn load_rgba_frames(path: &Path) -> Result<Vec<image::RgbaImage>, String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" => {
            let img = image::open(path).map_err(|e| e.to_string())?;
            Ok(vec![img.to_rgba8()])
        }
        "gif" => {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut decoder = gif::DecodeOptions::new();
            decoder.set_color_output(gif::ColorOutput::RGBA);
            let mut decoder = decoder.read_info(file).map_err(|e| e.to_string())?;
            let mut frames = Vec::new();
            while let Some(frame) = decoder.read_next_frame().map_err(|e| e.to_string())? {
                let buf = frame.buffer.clone().into_owned();
                frames.push(image::RgbaImage::from_raw(decoder.width() as u32, decoder.height() as u32, buf).ok_or("GIF帧解码失败")?);
            }
            if frames.is_empty() { return Err("GIF没有帧".into()); }
            Ok(frames)
        }
        "apng" => {
            // 简化：暂用首帧作为静态图导入
            let img = image::open(path).map_err(|e| e.to_string())?;
            Ok(vec![img.to_rgba8()])
        }
        _ => Err("不支持的图片扩展名".into()),
    }
}


