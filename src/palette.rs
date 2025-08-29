use eframe::egui::Color32;
use rust_embed::RustEmbed;

#[derive(Clone)]
pub struct Palette {
    pub colors: [Color32; 256],
}

impl Palette {
    /// 灰度默认调色板：用于兜底或缺省展示
    pub fn default_grayscale() -> Self {
        let mut arr = [Color32::BLACK; 256];
        for i in 0..256u32 {
            let v = i as u8;
            arr[i as usize] = Color32::from_rgb(v, v, v);
        }
        Self { colors: arr }
    }

    /// 从 `.pal` 的 768 字节（RGB*256）构建调色板
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 256 * 3 { return Err("PAL字节数不足".into()); }
        let mut arr = [Color32::BLACK; 256];
        for i in 0..256usize {
            let r = bytes[i * 3];
            let g = bytes[i * 3 + 1];
            let b = bytes[i * 3 + 2];
            arr[i] = Color32::from_rgb(r, g, b);
        }
        Ok(Self { colors: arr })
    }

    /// 转为 `.pal` 字节序列（RGB*256）
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 * 3);
        for c in &self.colors {
            out.push(c.r());
            out.push(c.g());
            out.push(c.b());
        }
        out
    }

    #[allow(dead_code)]
    pub fn from_directory(dir: &std::path::Path) -> Vec<(String, Self)> {
        let mut v = Vec::new();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() { // 递归一层目录结构 Palettes/RA2/xxx.pal
                    v.extend(Self::from_directory(&p));
                    continue;
                }
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
                if ext == "pal" {
                    if let Ok(bytes) = std::fs::read(&p) {
                        if let Ok(pal) = Self::from_bytes(&bytes) {
                            let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("PAL").to_string();
                            v.push((name, pal));
                        }
                    }
                }
            }
        }
        v
    }
}

#[derive(RustEmbed)]
#[folder = "Palettes"]
pub struct EmbeddedPalettes;

impl EmbeddedPalettes {
    /// 将内嵌的 `.pal` 资源按文件夹分组：返回 (目录, [(名称, Palette)])
    pub fn grouped_by_folder() -> Vec<(String, Vec<(String, Palette)>)> {
        let mut groups: std::collections::BTreeMap<String, Vec<(String, Palette)>> = std::collections::BTreeMap::new();
        for f in EmbeddedPalettes::iter() {
            let path = f.as_ref();
            if !path.to_ascii_lowercase().ends_with(".pal") { continue; }
            if let Some(file) = EmbeddedPalettes::get(path) {
                if let Ok(pal) = Palette::from_bytes(file.data.as_ref()) {
                    let p = std::path::Path::new(path);
                    let folder = p.parent().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("PAL").to_string();
                    groups.entry(folder).or_default().push((name, pal));
                }
            }
        }
        groups.into_iter().collect()
    }
}


