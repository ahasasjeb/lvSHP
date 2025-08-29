use eframe::egui::Color32;

// 颜色匹配：在 256 色调色板中寻找与目标 RGB 距离最小的索引
// 用于将 RGBA 图片量化到当前调色板（SHP 使用 8-bit palette 索引）
// 简化：使用欧氏距离平方（不含开方，性能更好）
#[inline]
fn dist_rgb2(a: Color32, b: Color32) -> u32 {
    let dr = a.r() as i32 - b.r() as i32;
    let dg = a.g() as i32 - b.g() as i32;
    let db = a.b() as i32 - b.b() as i32;
    (dr * dr + dg * dg + db * db) as u32
}

/// 在 `palette` 中返回与 `color` 最接近的调色板索引
pub fn best_index_rgb(color: Color32, palette: &[Color32; 256]) -> u8 {
    let mut best = 0u8;
    let mut best_d = u32::MAX;
    for i in 0..256u16 {
        let d = dist_rgb2(color, palette[i as usize]);
        if d < best_d { best_d = d; best = i as u8; if d == 0 { break; } }
    }
    best
}


