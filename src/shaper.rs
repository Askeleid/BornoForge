use harfrust::{Buffer, Font, ShapeOptions, ShaperFont, shape};
use read_fonts::types::GlyphId;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub cluster: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

pub struct Shaper {
    font_bytes: &'static [u8],
    shaper_font: ShaperFont<'static, 'static>,
}

impl Shaper {
    /// Pre-parses the OpenType tables once so `shape_text` has zero setup overhead.
    pub fn new(bytes: Vec<u8>) -> Self {
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let font: &'static Font = Box::leak(Box::new(
            Font::new(leaked, 0).expect("Failed to parse OpenType font"),
        ));
        let shaper_font = ShaperFont::new(font);

        Self {
            font_bytes: leaked,
            shaper_font,
        }
    }

    pub fn font_bytes(&self) -> &[u8] {
        self.font_bytes
    }

    /// Shapes a UTF-8 string using the pre-parsed ShaperFont.
    pub fn shape_text(&self, text: &str) -> Vec<ShapedGlyph> {
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();

        shape(&self.shaper_font, &mut buffer, ShapeOptions::new())
            .expect("HarfRust shaping failed");

        let infos = buffer.glyph_infos();
        let positions = buffer.glyph_positions();

        infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                glyph_id: info.glyph_id,
                cluster: info.cluster,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
            })
            .collect()
    }

    /// Collects all final shaped GlyphIds across all phrases.
    pub fn collect_required_gids(&self, phrases: &[String]) -> BTreeSet<GlyphId> {
        let mut gids = BTreeSet::new();
        for phrase in phrases {
            for g in self.shape_text(phrase) {
                gids.insert(GlyphId::new(g.glyph_id));
            }
        }
        gids
    }
}