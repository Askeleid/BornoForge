use crate::shaper::Shaper;

/// Compares every shaped glyph run between the original font and subset font.
/// If a sentence fails, it automatically isolates the exact word that broke.
pub fn validate_subset(
    orig_shaper: &Shaper,
    sub_shaper: &Shaper,
    phrases: &[String],
) -> Result<usize, String> {
    let mut total_glyphs = 0;

    for phrase in phrases {
        let orig_run = orig_shaper.shape_text(phrase);
        let sub_run = sub_shaper.shape_text(phrase);

        if orig_run.len() != sub_run.len() || !runs_match(&orig_run, &sub_run) {
            // Isolate which specific word inside the sentence failed!
            for word in phrase.split_whitespace() {
                let ow = orig_shaper.shape_text(word);
                let sw = sub_shaper.shape_text(word);
                if ow.len() != sw.len() || !runs_match(&ow, &sw) {
                    let mut details = format!(
                        "Word '{}' failed (orig={} glyphs vs subset={} glyphs):\n",
                        word,
                        ow.len(),
                        sw.len()
                    );
                    details.push_str(&format!("      Orig  : {:?}\n", ow));
                    details.push_str(&format!("      Subset: {:?}", sw));
                    return Err(details);
                }
            }

            return Err(format!(
                "Phrase mismatch in '{}': original={} vs subset={}",
                phrase,
                orig_run.len(),
                sub_run.len()
            ));
        }

        total_glyphs += sub_run.len();
    }

    Ok(total_glyphs)
}

fn runs_match(orig: &[crate::shaper::ShapedGlyph], sub: &[crate::shaper::ShapedGlyph]) -> bool {
    orig.len() == sub.len()
        && orig.iter().zip(sub.iter()).all(|(o, s)| {
            o.x_advance == s.x_advance
                && o.y_advance == s.y_advance
                && o.x_offset == s.x_offset
                && o.y_offset == s.y_offset
                && o.cluster == s.cluster
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsetter::{SubsetMode, build_subset_font};
    use std::fs;

    #[test]
    fn test_both_modes_preserve_complex_bengali_shaping() {
        let font_bytes = fs::read("assets/NotoSansBengali-Regular.ttf")
            .expect("Place NotoSansBengali-Regular.ttf in assets/ to run tests");
        let orig_shaper = Shaper::new(font_bytes);

        let corpus = vec![
            "বিজ্ঞান".to_string(),
            "লক্ষ্মী".to_string(),
            "র‍্যাব".to_string(),
            "কিংকর্তব্যবিমূঢ়".to_string(),
            "মৃত্যুঞ্জয়".to_string(),
        ];

        for mode in [SubsetMode::Dynamic, SubsetMode::Static] {
            let sub_bytes = build_subset_font(&orig_shaper, &corpus, mode);
            let sub_shaper = Shaper::new(sub_bytes);
            let res = validate_subset(&orig_shaper, &sub_shaper, &corpus);
            assert!(res.is_ok(), "Mode {:?} failed validation: {:?}", mode, res.err());
        }
    }
}