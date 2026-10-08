//! Exact file glyph outlines, without registering or installing a system font.
use std::{fmt::Write, io::Read, path::Path};
#[derive(Debug)]
pub struct Specimen {
    pub family: String,
    pub glyphs: u16,
    pub missing: bool,
    pub svg: Vec<u8>,
}
pub fn load(root: &Path, path: &Path, color: &str) -> Result<Specimen, String> {
    let file =
        std::fs::File::open(crate::files::resolve(root, path)?).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Not a font file.".into());
    }
    const LIMIT: usize = 16 * 1024 * 1024;
    let mut bytes = Vec::new();
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > LIMIT {
        return Err("Font exceeds the 16 MiB preview limit.".into());
    }
    if bytes.starts_with(b"wOF2") || bytes.starts_with(b"wOFF") {
        let expanded = bytes
            .get(16..20)
            .and_then(|v| v.try_into().ok())
            .map(u32::from_be_bytes)
            .ok_or("Invalid web font header")?;
        if expanded > 32 * 1024 * 1024 {
            return Err("Expanded font exceeds the preview limit.".into());
        }
        bytes = if bytes.starts_with(b"wOF2") {
            wuff::decompress_woff2(&bytes)
        } else {
            wuff::decompress_woff1(&bytes)
        }
        .map_err(|e| format!("Cannot decode web font: {e:?}"))?;
    }
    specimen(&bytes, color)
}
pub fn specimen(bytes: &[u8], color: &str) -> Result<Specimen, String> {
    let face = ttf_parser::Face::parse(bytes, 0).map_err(|e| format!("Invalid font: {e:?}"))?;
    let family = face
        .names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
        .and_then(|n| n.to_string())
        .unwrap_or_else(|| "Font preview".into());
    let mut available = std::collections::BTreeSet::new();
    if let Some(cmap) = face.tables().cmap {
        for table in cmap.subtables {
            if table.is_unicode() {
                table.codepoints(|code| {
                    if let Some(c) =
                        char::from_u32(code).filter(|c| !c.is_control() && !c.is_whitespace())
                        && available.len() < 256
                        && face.glyph_index(c).is_some()
                    {
                        available.insert(c);
                    }
                });
            }
        }
    }
    let coverage: String = available.into_iter().take(70).collect();
    let samples = [
        ("The quick brown fox", 48.),
        ("Zażółć gęślą jaźń", 36.),
        ("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 24.),
        ("abcdefghijklmnopqrstuvwxyz 0123456789", 24.),
        (coverage.as_str(), 20.),
    ];
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"390\"><g fill=\"{color}\">"
    );
    let mut missing = false;
    for (line, (text, size)) in samples.iter().enumerate() {
        let scale = size / face.units_per_em() as f32;
        let mut x = 20.;
        let y = 70. + line as f32 * 68.;
        for c in text.chars() {
            if let Some(glyph) = face.glyph_index(c) {
                let mut outline = Outline(String::new());
                face.outline_glyph(glyph, &mut outline);
                if !outline.0.is_empty() {
                    let _ = write!(
                        svg,
                        "<path transform=\"translate({x} {y}) scale({scale} {})\" d=\"{}\"/>",
                        -scale, outline.0
                    );
                }
                x += face.glyph_hor_advance(glyph).unwrap_or(0) as f32 * scale;
            } else {
                missing = true;
                x += size * 0.5;
            }
            if x > 960. {
                break;
            }
        }
    }
    svg.push_str("</g></svg>");
    Ok(Specimen {
        family,
        glyphs: face.number_of_glyphs(),
        missing,
        svg: svg.into_bytes(),
    })
}
struct Outline(String);
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "M{x} {y} ");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.0, "L{x} {y} ");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.0, "Q{x1} {y1} {x} {y} ");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y} ");
    }
    fn close(&mut self) {
        self.0.push_str("Z ");
    }
}
