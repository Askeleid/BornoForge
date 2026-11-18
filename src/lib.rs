use harfrust::{shape, Buffer, Font, ShapeOptions, ShaperFont};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use fontdue::{Font as RasterFont, FontSettings};
use image::{ImageBuffer, Rgba};

#[derive(Serialize)]
struct GlyphInfo {
    id: u32,
    cluster: u32,
    x_advance: i32,
    y_advance: i32,
    x_offset: i32,
    y_offset: i32,
}

#[derive(Serialize)]
struct ShapeResult {
    glyphs: Vec<GlyphInfo>,
}

#[wasm_bindgen]
pub fn shape_bengali(font_data: &[u8], text: &str) -> Result<String, JsValue> {
    let font = Font::new(font_data.to_vec(), 0)
    .ok_or_else(|| JsValue::from_str("Could not load font"))?;
    let shaper_font = ShaperFont::new(&font);

    let mut buffer = Buffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();

    shape(&shaper_font, &mut buffer, ShapeOptions::new())
        .map_err(|e| JsValue::from_str(&format!("Shaping failed: {e:?}")))?;

    let glyphs = buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions())
        .map(|(info, position)| GlyphInfo {
            id: info.glyph_id,
            cluster: info.cluster,
            x_advance: position.x_advance,
            y_advance: position.y_advance,
            x_offset: position.x_offset,
            y_offset: position.y_offset,
        })
        .collect();

    serde_json::to_string(&ShapeResult { glyphs })
        .map_err(|e| JsValue::from_str(&format!("Serialization failed: {e}")))
}

#[wasm_bindgen]
pub fn render_shaped(
    font_data: &[u8],
    text: &str,
) -> Result<Vec<u8>, JsValue> {
    // Load the font for shaping.
    let font = Font::new(font_data.to_vec(), 0)
        .ok_or_else(|| JsValue::from_str("Could not load shaping font"))?;

    let shaper_font = ShaperFont::new(&font);

    // Load the same font for glyph rasterization.
    let raster_font = RasterFont::from_bytes(
        font_data,
        FontSettings::default(),
    )
    .map_err(|e| JsValue::from_str(&format!("Raster font loading failed: {e}")))?;

    // Shape the input text.
    let mut buffer = Buffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();

    shape(&shaper_font, &mut buffer, ShapeOptions::new())
        .map_err(|e| JsValue::from_str(&format!("Shaping failed: {e:?}")))?;

    // Rendering configuration.
    let font_size = 72.0;
    let scale = font_size / 1000.0;
    let width = 1200;
    let height = 180;

    let baseline = 120.0;
    let mut cursor_x: f32 = 40.0;

    let mut image = ImageBuffer::from_pixel(
        width,
        height,
        Rgba([13, 17, 23, 255]),
    );

    // Rasterize and position each shaped glyph.
    for (info, position) in buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions())
    {
        let glyph_id = u16::try_from(info.glyph_id)
            .map_err(|_| JsValue::from_str("Invalid glyph ID"))?;

        let (metrics, bitmap) =
            raster_font.rasterize_indexed(glyph_id, font_size);

        let glyph_x = cursor_x + position.x_offset as f32 * scale;
        let glyph_y = baseline - position.y_offset as f32 * scale;

        let origin_x = glyph_x.floor() as i32 + metrics.xmin;
        let origin_y = glyph_y.floor() as i32 - metrics.height as i32 - metrics.ymin;

        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let alpha = bitmap[y * metrics.width + x];

                if alpha == 0 {
                    continue;
                }

                let px = origin_x + x as i32;
                let py = origin_y + y as i32;

                if px < 0 || py < 0 || px >= width as i32 || py >= height as i32 {
                    continue;
                }

                image.put_pixel(
                    px as u32,
                    py as u32,
                    Rgba([240, 246, 252, alpha]),
                );
            }
        }

        cursor_x += position.x_advance as f32 * scale;
    }

    // Encode the rendered image as PNG.
    let mut png_data = Vec::new();

    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        )
        .map_err(|e| JsValue::from_str(&format!("PNG encoding failed: {e}")))?;

    Ok(png_data)
}


#[wasm_bindgen]
pub fn render_unshaped(
    font_data: &[u8],
    text: &str,
) -> Result<Vec<u8>, JsValue> {
    // Load the font for direct glyph rasterization.
    let raster_font = RasterFont::from_bytes(
        font_data,
        FontSettings::default(),
    )
    .map_err(|e| JsValue::from_str(&format!("Raster font loading failed: {e}")))?;

    // Rendering configuration.
    let font_size = 72.0;
    let width = 1200;
    let height = 180;

    let baseline = 120.0;
    let mut cursor_x: f32 = 40.0;

    let mut image = ImageBuffer::from_pixel(
        width,
        height,
        Rgba([13, 17, 23, 255]),
    );

    // Render each Unicode character independently.
    for character in text.chars() {
        let glyph_id = raster_font.lookup_glyph_index(character);

        let (metrics, bitmap) =
            raster_font.rasterize_indexed(glyph_id, font_size);

        let origin_x = cursor_x.floor() as i32 + metrics.xmin;
        let origin_y =
            baseline as i32 - metrics.height as i32 - metrics.ymin;

        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let alpha = bitmap[y * metrics.width + x];

                if alpha == 0 {
                    continue;
                }

                let px = origin_x + x as i32;
                let py = origin_y + y as i32;

                if px < 0 || py < 0
                    || px >= width as i32
                    || py >= height as i32
                {
                    continue;
                }

                image.put_pixel(
                    px as u32,
                    py as u32,
                    Rgba([240, 246, 252, alpha]),
                );
            }
        }

        cursor_x += metrics.advance_width;
    }

    // Encode the rendered image as PNG.
    let mut png_data = Vec::new();

    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut png_data),
            image::ImageFormat::Png,
        )
        .map_err(|e| JsValue::from_str(&format!("PNG encoding failed: {e}")))?;

    Ok(png_data)
}
