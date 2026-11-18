use crate::shaper::Shaper;
use read_fonts::TableProvider;
use clap::ValueEnum;
use read_fonts::{
    FontRef,
    collections::int_set::IntSet,
    types::{GlyphId, NameId, Tag},
};
use skera::{Plan, SubsetFlags, subset_font};
use std::collections::BTreeSet;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum SubsetMode {
    /// Retains full OpenType GSUB closure for all used characters (safe for dynamic input/chat)
    Dynamic,
    /// Retains ONLY the exact glyphs and in-flight GSUB intermediates needed for the static script
    Static,
}

pub fn count_unique_glyphs(font_bytes: &[u8]) -> u16 {
    let font_ref = FontRef::new(font_bytes).expect("Failed to parse font");
    font_ref.maxp().map(|m| m.num_glyphs()).unwrap_or(0)
}

pub fn build_subset_font(shaper: &Shaper, phrases: &[String], mode: SubsetMode) -> Vec<u8> {
    let font_bytes = shaper.font_bytes();
    let font_ref = FontRef::new(font_bytes).expect("Failed to read FontRef");

    // 1. Collect all unique Unicode codepoints + Bengali canonical decompositions
    let mut unicodes = IntSet::<u32>::empty();
    for phrase in phrases {
        for ch in phrase.chars() {
            insert_char_with_decompositions(&mut unicodes, ch);
        }
    }

    // 2. Collect final shaped GlyphIds from HarfRust
    let mut gids = IntSet::<GlyphId>::empty();
    for gid in shaper.collect_required_gids(phrases) {
        gids.insert(gid);
    }

    let drop_tables = default_drop_tables();
    let keep_names = default_keep_names();

    if mode == SubsetMode::Dynamic {
        let plan = Plan::new(
            &gids,
            &unicodes,
            &font_ref,
            SubsetFlags::default(),
            &drop_tables,
            &IntSet::all(),
            &IntSet::all(),
            &keep_names,
            &IntSet::empty(),
        );
        return subset_font(&font_ref, &plan).expect("Dynamic subsetting failed");
    }

    // --- STATIC MODE (NO_LAYOUT_CLOSURE + Isolated Akshara Micro-Closure) ---
    let static_flags = SubsetFlags::SUBSET_FLAGS_NO_LAYOUT_CLOSURE;
    let plan = Plan::new(
        &gids,
        &unicodes,
        &font_ref,
        static_flags,
        &drop_tables,
        &IntSet::all(),
        &IntSet::all(),
        &keep_names,
        &IntSet::empty(),
    );
    let candidate_bytes = subset_font(&font_ref, &plan).expect("Initial static subsetting failed");
    let candidate_shaper = Shaper::new(candidate_bytes.clone());

    // Extract all unique multi-character Bengali syllables (Aksharas) across the entire script.
    // Single characters never have in-flight GSUB intermediates, so we skip them!
    let mut unique_syllables = BTreeSet::new();
    for phrase in phrases {
        for word in phrase.split_whitespace() {
            // Trim ASCII and Bengali punctuation ('।') so "টি!" and "টি," deduplicate to "টি"
            let clean_word = word.trim_matches(|c: char| {
                c.is_ascii_punctuation() || matches!(c, '।' | '—' | '–' | '‘' | '’' | '“' | '”' | '…')
            });
            for syl in split_bengali_syllables(clean_word) {
                if syl.chars().count() >= 2 {
                    unique_syllables.insert(syl);
                }
            }
        }
    }

    let mut healed = false;
    let mut seen_signatures = BTreeSet::new();

    for syl in &unique_syllables {
        let orig_run = shaper.shape_text(syl);
        let sub_run = candidate_shaper.shape_text(syl);

        let matches = orig_run.len() == sub_run.len()
            && orig_run.iter().zip(sub_run.iter()).all(|(o, s)| {
                o.x_advance == s.x_advance
                    && o.y_advance == s.y_advance
                    && o.x_offset == s.x_offset
                    && o.y_offset == s.y_offset
                    && o.cluster == s.cluster
            });

        if !matches {
            // Create a canonical character-set signature for this syllable so we never run
            // identical Skera closures twice (e.g., punctured/repeated syllable variants).
            let mut sig: Vec<char> = syl
                .chars()
                .filter(|c| (*c as u32) >= 0x0980 && (*c as u32) <= 0x09FF)
                .collect();
            sig.sort_unstable();
            sig.dedup();

            if seen_signatures.insert(sig) {
                for gid in discover_single_syllable_gids(&font_ref, shaper, syl) {
                    gids.insert(gid);
                }
                healed = true;
            }
        }
    }

    if !healed {
        return candidate_bytes;
    }

    let healed_plan = Plan::new(
        &gids,
        &unicodes,
        &font_ref,
        static_flags,
        &drop_tables,
        &IntSet::all(),
        &IntSet::all(),
        &keep_names,
        &IntSet::empty(),
    );
    subset_font(&font_ref, &healed_plan).expect("Healed static subsetting failed")
}

/// Runs Skera's layout closure on a single isolated syllable so no cross-syllable conjuncts leak in.
fn discover_single_syllable_gids(font_ref: &FontRef, orig_shaper: &Shaper, syl: &str) -> Vec<GlyphId> {
    let mut syl_unicodes = IntSet::<u32>::empty();
    let mut syl_gids = IntSet::<GlyphId>::empty();

    for ch in syl.chars() {
        insert_char_with_decompositions(&mut syl_unicodes, ch);
    }
    for g in orig_shaper.shape_text(syl) {
        syl_gids.insert(GlyphId::new(g.glyph_id));
    }

    let mini_plan = Plan::new(
        &syl_gids,
        &syl_unicodes,
        font_ref,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS,
        &default_drop_tables(),
        &IntSet::all(),
        &IntSet::all(),
        &IntSet::empty(),
        &IntSet::empty(),
    );

    let mini_font = subset_font(font_ref, &mini_plan).expect("Single syllable closure failed");
    extract_active_gids_from_loca(&mini_font)
}

/// Groups a Bengali word into orthographic syllables (Aksharas) so conjuncts (C + ্ + C)
/// and their dependent vowel signs (Kar / Nukta / Chandrabindu) stay together as one unit.
fn split_bengali_syllables(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut syllables = Vec::new();
    let mut current = String::new();

    for i in 0..chars.len() {
        let ch = chars[i];
        let prev_is_halant_or_zwj =
            i > 0 && matches!(chars[i - 1], '\u{09CD}' | '\u{200D}' | '\u{200C}');

        let is_dependent_mark = matches!(
            ch as u32,
            0x0981..=0x0983 // Chandrabindu, Anusvara, Visarga
            | 0x09BC        // Nukta
            | 0x09BE..=0x09CD // Dependent Vowel Kars & Hasanta (্)
            | 0x09D7        // Au length mark
            | 0x200C..=0x200D // ZWNJ / ZWJ
        );

        if !current.is_empty() && !is_dependent_mark && !prev_is_halant_or_zwj {
            syllables.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }

    if !current.is_empty() {
        syllables.push(current);
    }

    syllables
}

/// Inspects a RETAIN_GIDS subset font's `maxp` and `loca` tables to find all non-empty Original GIDs.
fn extract_active_gids_from_loca(font_bytes: &[u8]) -> Vec<GlyphId> {
    let font_ref = FontRef::new(font_bytes).expect("Failed to parse mini subset font");
    let mut maxp_slice: Option<&[u8]> = None;
    let mut loca_slice: Option<&[u8]> = None;

    for rec in font_ref.table_directory.table_records() {
        let start = rec.offset() as usize;
        let end = start + rec.length() as usize;
        if rec.tag() == Tag::new(b"maxp") {
            maxp_slice = font_bytes.get(start..end);
        } else if rec.tag() == Tag::new(b"loca") {
            loca_slice = font_bytes.get(start..end);
        }
    }

    let (Some(maxp), Some(loca)) = (maxp_slice, loca_slice) else {
        return Vec::new();
    };

    if maxp.len() < 6 {
        return Vec::new();
    }
    let num_glyphs = u16::from_be_bytes([maxp[4], maxp[5]]) as usize;
    let mut active = Vec::new();

    if loca.len() == (num_glyphs + 1) * 2 {
        for gid in 0..num_glyphs {
            let o1 = u16::from_be_bytes([loca[gid * 2], loca[gid * 2 + 1]]);
            let o2 = u16::from_be_bytes([loca[(gid + 1) * 2], loca[(gid + 1) * 2 + 1]]);
            if o1 != o2 {
                active.push(GlyphId::new(gid as u32));
            }
        }
    } else if loca.len() == (num_glyphs + 1) * 4 {
        for gid in 0..num_glyphs {
            let o1 = u32::from_be_bytes([
                loca[gid * 4],
                loca[gid * 4 + 1],
                loca[gid * 4 + 2],
                loca[gid * 4 + 3],
            ]);
            let o2 = u32::from_be_bytes([
                loca[(gid + 1) * 4],
                loca[(gid + 1) * 4 + 1],
                loca[(gid + 1) * 4 + 2],
                loca[(gid + 1) * 4 + 3],
            ]);
            if o1 != o2 {
                active.push(GlyphId::new(gid as u32));
            }
        }
    }

    active
}

fn insert_char_with_decompositions(unicodes: &mut IntSet<u32>, ch: char) {
    unicodes.insert(ch as u32);
    match ch {
        'ো' => { unicodes.insert('ে' as u32); unicodes.insert('া' as u32); }
        'ৌ' => { unicodes.insert('ে' as u32); unicodes.insert('\u{09D7}' as u32); }
        'ড়' => { unicodes.insert('ড' as u32); unicodes.insert('\u{09BC}' as u32); }
        'ঢ়' => { unicodes.insert('ঢ' as u32); unicodes.insert('\u{09BC}' as u32); }
        'য়' => { unicodes.insert('য' as u32); unicodes.insert('\u{09BC}' as u32); }
        _ => {}
    }
}

fn default_drop_tables() -> IntSet<Tag> {
    let mut drop_tables = IntSet::<Tag>::empty();
    for tag in [
        b"fpgm", b"prep", b"cvt ", b"gasp", b"DSIG",
        b"gvar", b"HVAR", b"fvar", b"avar", b"STAT",
    ] {
        drop_tables.insert(Tag::new(tag));
    }
    drop_tables
}

fn default_keep_names() -> IntSet<NameId> {
    let mut keep_names = IntSet::<NameId>::empty();
    for id in [1, 2, 4, 6] {
        keep_names.insert(NameId::new(id));
    }
    keep_names
}

pub fn inspect_font_tables(label: &str, font_bytes: &[u8]) {
    let font_ref = FontRef::new(font_bytes).expect("Failed to parse font");
    println!("--- {} ({} bytes total) ---", label, font_bytes.len());
    for record in font_ref.table_directory.table_records() {
        println!("  Table '{}' : {:6} bytes", record.tag(), record.length());
    }
    println!();
}