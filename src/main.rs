mod scanner;
mod shaper;
mod subsetter;
mod validator;

use clap::Parser;
use shaper::Shaper;
use std::{collections::BTreeSet, fs, time::Instant};
use subsetter::{SubsetMode, build_subset_font, inspect_font_tables};
use validator::validate_subset;

/// bgl10n: Universal Bengali Game Localization & Font Optimizer
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
    /// Path to the input localization file (.json, .csv, or .txt)
    #[arg(short, long)]
    locale: String,

    /// Path to the source Bengali OpenType font (.ttf or .otf)
    #[arg(short, long)]
    font: String,

    /// Path to write the optimized subset font
    #[arg(short, long)]
    output: String,

    /// Optimization mode: 'dynamic' (full closure) or 'static' (strict script-only)
    #[arg(short, long, value_enum, default_value_t = SubsetMode::Dynamic)]
    mode: SubsetMode,

    /// Print detailed OpenType table size breakdown
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() {
    let args = CliArgs::parse();
    let timer = Instant::now();

    let orig_bytes = fs::read(&args.font)
        .unwrap_or_else(|_| panic!("Could not find font file: {}", args.font));
    let orig_len = orig_bytes.len();
    let orig_shaper = Shaper::new(orig_bytes);

    // 1. Extract all dialogue strings (from a single file or entire directory)
    let (phrases, files_scanned) = scanner::extract_locale_strings(&args.locale);
    let unique_chars: BTreeSet<char> = phrases.iter().flat_map(|s| s.chars()).collect();

    // 2. Build the subset font in the requested mode
    let subset_vec = build_subset_font(&orig_shaper, &phrases, args.mode);
    fs::write(&args.output, &subset_vec).expect("Failed to write output subset font");
    let subset_len = subset_vec.len();
    let sub_shaper = Shaper::new(subset_vec);

    if args.verbose {
        inspect_font_tables("ORIGINAL FONT", orig_shaper.font_bytes());
        inspect_font_tables(&format!("SUBSET FONT ({:?})", args.mode), sub_shaper.font_bytes());
    }

    // 3. Run the Shaping-Diff CI Gate
    let verified_glyphs = match validate_subset(&orig_shaper, &sub_shaper, &phrases) {
        Ok(count) => count,
        Err(err) => {
            eprintln!("❌ SHAPING DIFF VALIDATION FAILED ({:?} mode):\n   {}", args.mode, err);
            std::process::exit(1);
        }
    };

    let elapsed = timer.elapsed();
    let reduction = 100.0 * (1.0 - (subset_len as f64 / orig_len as f64));
    let orig_glyphs = subsetter::count_unique_glyphs(orig_shaper.font_bytes());
    let sub_glyphs = subsetter::count_unique_glyphs(sub_shaper.font_bytes());

    println!("====================================================");
    println!(" bgl10n - Bengali Game Font Optimizer & Validator");
    println!("====================================================");
    println!("Optimization Mode : {:?}", args.mode);
    println!(
        "Locale Source     : {} ({} file(s), {} strings, {} unique chars)",
        args.locale,
        files_scanned,
        phrases.len(),
        unique_chars.len()
    );
    println!("Original Font     : {:.2} KB ({} bytes)", orig_len as f64 / 1024.0, orig_len);
    println!("Subset Font       : {:.2} KB ({} bytes) [-{:.1}%]", subset_len as f64 / 1024.0, subset_len, reduction);
    println!("Output Saved To   : {}", args.output);
    println!("Unique Glyphs     : {} / {}", sub_glyphs, orig_glyphs);
    println!("Shaping Diff Gate : PASSED ({} shaped glyphs verified across {} strings)", verified_glyphs, phrases.len());
    println!("Build Time        : {:.2?}", elapsed);
    println!("====================================================");   

}