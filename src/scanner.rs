use serde_json::Value;
use std::{fs, path::Path};

/// Extracts localized strings from a single file OR recursively from a directory
/// containing .json, .csv, .txt, .po, or .yarn localization files.
pub fn extract_locale_strings(path_str: &str) -> (Vec<String>, usize) {
    let path = Path::new(path_str);
    if !path.exists() {
        panic!("Locale path does not exist: {}", path_str);
    }

    let mut strings = Vec::new();
    let mut files_scanned = 0;

    if path.is_dir() {
        scan_directory_recursive(path, &mut strings, &mut files_scanned);
    } else {
        extract_from_single_file(path, &mut strings);
        files_scanned = 1;
    }

    (strings, files_scanned)
}

fn scan_directory_recursive(dir: &Path, out: &mut Vec<String>, files_scanned: &mut usize) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("Failed to read directory: {}", dir.display()));

    let mut paths: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort(); // Deterministic file order

    for path in paths {
        if path.is_dir() {
            scan_directory_recursive(&path, out, files_scanned);
        } else if is_supported_locale_file(&path) {
            extract_from_single_file(&path, out);
            *files_scanned += 1;
        }
    }
}

fn is_supported_locale_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase()).as_deref(),
        Some("json" | "txt" | "csv" | "po" | "yarn" | "xml" | "srt")
    )
}

fn extract_from_single_file(path: &Path, out: &mut Vec<String>) {
    let raw = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return, // Skip binary or non-UTF8 files gracefully
    };

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "json" => {
            if let Ok(root) = serde_json::from_str::<Value>(&raw) {
                collect_json_strings(&root, out);
            }
        }
        "srt" => {
            for line in raw.lines() {
                let trimmed = line.trim().trim_start_matches('\u{feff}'); // Strip UTF-8 BOM if present
                // Skip empty lines, pure subtitle index numbers, and timestamp lines
                if trimmed.is_empty()
                    || trimmed.chars().all(|c| c.is_ascii_digit())
                    || trimmed.contains("-->")
                {
                    continue;
                }
                // Strip common SRT formatting tags like <i>, </i>, <b>, </b>
                let clean = trimmed
                    .replace("<i>", "")
                    .replace("</i>", "")
                    .replace("<b>", "")
                    .replace("</b>", "");
                if !clean.trim().is_empty() {
                    out.push(clean.trim().to_string());
                }
            }
        }
        _ => {
            for line in raw.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
    }
}

fn collect_json_strings(val: &Value, out: &mut Vec<String>) {
    match val {
        Value::String(s) => {
            if !s.trim().is_empty() {
                out.push(s.clone());
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_json_strings(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_json_strings(v, out);
            }
        }
        _ => {}
    }
}