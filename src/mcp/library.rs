use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Data types ──

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub duration: Option<f64>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub file_size: u64,
    pub modified: String,
    pub category: Option<String>,
    pub root_note: Option<String>,
    pub bpm: Option<f32>,
    pub channels: Option<u32>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub bpm_range: Option<(u32, u32)>,
    #[serde(default)]
    pub tempo_marking: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DirListing {
    pub path: String,
    pub subdirectories: Vec<LibraryEntry>,
    pub samples: Vec<LibraryEntry>,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub total_samples: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchResults {
    pub total_results: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub results: Vec<LibraryEntry>,
}

// ── SampleLibrary ──

#[derive(Serialize, Deserialize)]
pub struct SampleLibrary {
    #[serde(default)]
    pub roots: Vec<PathBuf>,
    #[serde(default)]
    cache: HashMap<PathBuf, LibraryEntry>,
    #[serde(default)]
    dir_cache: HashMap<PathBuf, Vec<PathBuf>>,
}

impl Default for SampleLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl SampleLibrary {
    pub fn new() -> Self {
        SampleLibrary {
            roots: Vec::new(),
            cache: HashMap::new(),
            dir_cache: HashMap::new(),
        }
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
        self.cache.clear();
        self.dir_cache.clear();
    }

    pub fn list_dir(
        &mut self,
        path: &Path,
        page: usize,
        page_size: usize,
        filter: Option<&DirFilter>,
    ) -> Result<DirListing, String> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Cannot access '{}': {}", path.display(), e))?;

        if !canonical.is_dir() {
            return Err(format!("Not a directory: '{}'", path.display()));
        }

        let entries = self.read_dir_entries(&canonical)?;

        let mut subdirs: Vec<LibraryEntry> = Vec::new();
        let mut samples: Vec<LibraryEntry> = Vec::new();

        for child_path in &entries {
            let entry = self.get_or_cache_entry(child_path);
            match entry {
                Ok(e) if e.is_directory => {
                    subdirs.push(e.clone());
                }
                Ok(e) => {
                    if let Some(f) = filter {
                        if !f.matches(&e) {
                            continue;
                        }
                    }
                    samples.push(e.clone());
                }
                Err(_) => {}
            }
        }

        subdirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        samples.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let total_samples = samples.len();
        let total_pages = if page_size == 0 {
            1
        } else {
            (total_samples + page_size - 1) / page_size
        };
        let start = page * page_size;
        let page_samples: Vec<LibraryEntry> = samples
            .into_iter()
            .skip(start)
            .take(page_size)
            .collect();

        Ok(DirListing {
            path: canonical.to_string_lossy().to_string(),
            subdirectories: subdirs,
            samples: page_samples,
            page,
            page_size,
            total_pages,
            total_samples,
        })
    }

    pub fn search(
        &self,
        query: &str,
        scope_roots: Option<&[PathBuf]>,
        page: usize,
        page_size: usize,
    ) -> SearchResults {
        let query_lower = query.to_lowercase();
        let mut results: Vec<LibraryEntry> = Vec::new();

        for entry in self.cache.values() {
            if entry.is_directory {
                continue;
            }
            if let Some(roots) = scope_roots {
                if !roots.iter().any(|r| entry.path.starts_with(r.to_string_lossy().as_ref())) {
                    continue;
                }
            }
            if entry.name.to_lowercase().contains(&query_lower) {
                results.push(entry.clone());
            }
            if let Some(ref cat) = entry.category {
                if cat.to_lowercase().contains(&query_lower) {
                    if !results.iter().any(|r| r.path == entry.path) {
                        results.push(entry.clone());
                    }
                }
            }
        }

        results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let total_results = results.len();
        let total_pages = if page_size == 0 {
            1
        } else {
            (total_results + page_size - 1) / page_size
        };
        let start = page * page_size;
        let page_results: Vec<LibraryEntry> = results.into_iter().skip(start).take(page_size).collect();

        SearchResults {
            total_results,
            page,
            page_size,
            total_pages,
            results: page_results,
        }
    }

    pub fn get_entry(&self, path: &Path) -> Option<&LibraryEntry> {
        self.cache.get(path)
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    // ── Persistence ──

    /// Serialize the library to a JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Deserialize the library from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid sample library cache: {e}"))
    }

    /// Save the library to a JSON file at `path`.
    ///
    /// Writes to a `.tmp` sibling file first, then atomically renames it
    /// into place. This avoids a half-written cache corrupting the file
    /// if the process crashes mid-write.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let json = self.to_json()?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json)
            .map_err(|e| format!("Cannot write sample library cache: {e}"))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("Cannot rename sample library cache: {e}"))?;
        Ok(())
    }

    /// Load the library from a JSON file at `path`.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read sample library cache: {e}"))?;
        Self::from_json(&json)
    }

    // ── Internal helpers ──

    fn read_dir_entries(&mut self, dir: &Path) -> Result<Vec<PathBuf>, String> {
        if let Some(cached) = self.dir_cache.get(dir) {
            return Ok(cached.clone());
        }

        let mut entries: Vec<PathBuf> = Vec::new();
        let rd = fs::read_dir(dir)
            .map_err(|e| format!("Cannot read directory '{}': {}", dir.display(), e))?;

        for result in rd {
            match result {
                Ok(entry) => {
                    entries.push(entry.path());
                }
                Err(e) => {
                    eprintln!("[mcp/library] Error reading entry in '{}': {}", dir.display(), e);
                }
            }
        }

        self.dir_cache.insert(dir.to_path_buf(), entries.clone());
        Ok(entries)
    }

    fn get_or_cache_entry(&mut self, path: &Path) -> Result<LibraryEntry, String> {
        if let Some(cached) = self.cache.get(path) {
            return Ok(cached.clone());
        }

        let entry = build_entry(path)?;
        self.cache.insert(path.to_path_buf(), entry.clone());
        Ok(entry)
    }
}

// ── Directory filter ──

#[derive(Default, Clone, Debug)]
pub struct DirFilter {
    pub name_contains: Option<String>,
    pub min_duration: Option<f64>,
    pub max_duration: Option<f64>,
    pub category: Option<String>,
    pub min_bpm: Option<f32>,
    pub max_bpm: Option<f32>,
    pub key: Option<String>,
    pub tag: Option<String>,
    pub channels_filter: Option<u8>,
}

impl DirFilter {
    pub fn matches(&self, entry: &LibraryEntry) -> bool {
        if let Some(ref needle) = self.name_contains {
            if !entry.name.to_lowercase().contains(&needle.to_lowercase()) {
                return false;
            }
        }
        if let Some(ref cat) = self.category {
            match entry.category {
                Some(ref c) => {
                    if !c.to_lowercase().contains(&cat.to_lowercase()) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        if let Some(min) = self.min_duration {
            if let Some(d) = entry.duration {
                if d < min {
                    return false;
                }
            }
        }
        if let Some(max) = self.max_duration {
            if let Some(d) = entry.duration {
                if d > max {
                    return false;
                }
            }
        }
        if let Some(min) = self.min_bpm {
            if let Some(b) = entry.bpm {
                if b < min {
                    return false;
                }
            }
        }
        if let Some(max) = self.max_bpm {
            if let Some(b) = entry.bpm {
                if b > max {
                    return false;
                }
            }
        }
        if let Some(ref needle) = self.key {
            let needle_lc = needle.to_lowercase();
            match entry.key {
                Some(ref k) => {
                    if !k.to_lowercase().contains(&needle_lc) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        if let Some(ref needle) = self.tag {
            let needle_lc = needle.to_lowercase();
            if !entry.tags.iter().any(|t| t.to_lowercase().contains(&needle_lc)) {
                return false;
            }
        }
        if let Some(ch) = self.channels_filter {
            match entry.channels {
                Some(c) => {
                    if c != ch as u32 {
                        return false;
                    }
                }
                None => return false,
            }
        }
        true
    }
}

// ── Build a LibraryEntry from a filesystem path ──

fn build_entry(path: &Path) -> Result<LibraryEntry, String> {
    let metadata = fs::metadata(path)
        .map_err(|e| format!("Cannot read metadata for '{}': {}", path.display(), e))?;

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let is_directory = metadata.is_dir();
    let file_size = metadata.len();

    let modified = match metadata.modified() {
        Ok(time) => {
            let secs = time
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            format_iso8601(secs)
        }
        Err(_) => String::new(),
    };

    let mut entry = LibraryEntry {
        name,
        path: path.to_string_lossy().to_string(),
        is_directory,
        duration: None,
        sample_rate: None,
        bit_depth: None,
        file_size,
        modified,
        category: None,
        root_note: None,
        bpm: None,
        channels: None,
        key: None,
        tags: Vec::new(),
        bpm_range: None,
        tempo_marking: None,
    };

    if !is_directory {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("wav") {
            if let Ok(meta) = read_wav_header(path) {
                entry.duration = Some(meta.duration);
                entry.sample_rate = Some(meta.sample_rate);
                entry.bit_depth = Some(meta.bits_per_sample);
                entry.channels = Some(meta.num_channels);
            }
        }
        let parsed = parse_filename(&entry.name);
        entry.category = parsed.category;
        entry.root_note = parsed.root_note;
        entry.bpm = parsed.bpm;
        entry.key = parsed.key;
        entry.tags = parsed.tags;
        entry.bpm_range = parsed.bpm_range;
        entry.tempo_marking = parsed.tempo_marking;
    }

    Ok(entry)
}

// ── WAV header reader (no PCM data) ──

struct WavMeta {
    duration: f64,
    sample_rate: u32,
    bits_per_sample: u8,
    num_channels: u32,
}

fn read_wav_header(path: &Path) -> Result<WavMeta, String> {
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Cannot open '{}': {}", path.display(), e))?;

    let mut buf = [0u8; 12];
    file.read_exact(&mut buf)
        .map_err(|_| "Cannot read RIFF header".to_string())?;

    if &buf[0..4] != b"RIFF" || &buf[8..12] != b"WAVE" {
        return Err("Not a WAV file".to_string());
    }

    let mut sample_rate: u32 = 0;
    let mut bits_per_sample: u32 = 0;
    let mut num_channels: u32 = 0;
    let mut data_size: u64 = 0;
    let mut found_fmt = false;
    let mut found_data = false;

    loop {
        let mut header = [0u8; 8];
        if file.read_exact(&mut header).is_err() {
            break;
        }
        let chunk_id = &header[0..4];
        let chunk_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        if chunk_id == b"fmt " {
            let mut fmt_buf = [0u8; 16];
            if file.read_exact(&mut fmt_buf).is_err() {
                break;
            }
            let audio_format = u16::from_le_bytes([fmt_buf[0], fmt_buf[1]]);
            if audio_format != 1 && audio_format != 3 {
                return Err("Unsupported WAV format (not PCM or IEEE float)".to_string());
            }
            num_channels = u32::from_le_bytes([fmt_buf[2], fmt_buf[3], 0, 0]);
            sample_rate = u32::from_le_bytes([fmt_buf[4], fmt_buf[5], fmt_buf[6], fmt_buf[7]]);
            let _byte_rate = u32::from_le_bytes([fmt_buf[8], fmt_buf[9], fmt_buf[10], fmt_buf[11]]);
            let _block_align = u16::from_le_bytes([fmt_buf[12], fmt_buf[13]]);
            bits_per_sample = u32::from_le_bytes([fmt_buf[14], fmt_buf[15], 0, 0]);
            found_fmt = true;

            if chunk_size > 16 {
                let skip = (chunk_size - 16) as i64;
                if file.seek(std::io::SeekFrom::Current(skip)).is_err() {
                    break;
                }
            }
        } else if chunk_id == b"data" {
            data_size = chunk_size as u64;
            found_data = true;
            break;
        } else {
            if file
                .seek(std::io::SeekFrom::Current(chunk_size as i64))
                .is_err()
            {
                break;
            }
        }
    }

    if found_fmt && found_data && sample_rate > 0 && num_channels > 0 && bits_per_sample > 0 {
        let bytes_per_sample = ((bits_per_sample / 8).max(1)) as u64;
        let total_samples = data_size / (bytes_per_sample * num_channels as u64);
        Ok(WavMeta {
            duration: total_samples as f64 / sample_rate as f64,
            sample_rate,
            bits_per_sample: bits_per_sample as u8,
            num_channels,
        })
    } else {
        Err("Incomplete WAV header".to_string())
    }
}

// ── Filename heuristic parser ──

#[derive(Debug, Default)]
struct ParsedMetadata {
    category: Option<String>,
    root_note: Option<String>,
    bpm: Option<f32>,
    key: Option<String>,
    tags: Vec<String>,
    bpm_range: Option<(u32, u32)>,
    tempo_marking: Option<String>,
}

fn parse_filename(name: &str) -> ParsedMetadata {
    let stem = if let Some(dot) = name.rfind('.') {
        &name[..dot]
    } else {
        name
    };

    let tokens: Vec<&str> = stem
        .split(|c: char| c == '_' || c == ' ' || c == '.')
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return ParsedMetadata::default();
    }

    let mut category_tokens: Vec<&str> = Vec::new();
    let mut root_note: Option<String> = None;
    let mut bpm: Option<f32> = None;
    let mut key: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut bpm_range: Option<(u32, u32)> = None;
    let mut tempo_marking: Option<String> = None;
    let mut found_musical = false;

    for token in &tokens {
        if let Some(k) = try_parse_key_token(token) {
            if key.is_none() {
                key = Some(k);
            }
            found_musical = true;
            continue;
        }
        if let Some(t) = try_parse_tempo_marking_token(token) {
            if tempo_marking.is_none() {
                tempo_marking = Some(t);
            }
            continue;
        }
        if let Some((lo, hi)) = try_parse_bpm_range_token(token) {
            if bpm_range.is_none() {
                bpm_range = Some((lo, hi));
            }
            continue;
        }
        if let Some(b) = try_parse_bpm_token(token) {
            if bpm.is_none() {
                bpm = Some(b);
            }
            found_musical = true;
            continue;
        }
        if bpm.is_none() && is_standalone_bpm(token) {
            bpm = Some(token.parse::<f32>().unwrap_or(0.0));
            found_musical = true;
            continue;
        }
        if let Some(note) = try_parse_note_token(token) {
            if root_note.is_none() {
                root_note = Some(note);
            }
            found_musical = true;
            continue;
        }
        if let Some(tag) = try_parse_tag_token(token) {
            tags.push(tag);
            continue;
        }
        category_tokens.push(token);
    }

    let category = if found_musical && !category_tokens.is_empty() {
        Some(category_tokens.join(" "))
    } else {
        None
    };

    let final_tags = if found_musical { tags } else { Vec::new() };

    ParsedMetadata {
        category,
        root_note,
        bpm,
        key,
        tags: final_tags,
        bpm_range,
        tempo_marking,
    }
}

fn try_parse_note_token(token: &str) -> Option<String> {
    let bytes = token.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0].to_ascii_uppercase();
    if first < b'A' || first > b'G' {
        return None;
    }
    let mut idx = 1;
    let mut modifier = "";
    if idx < bytes.len() {
        match bytes[idx] {
            b'#' => { modifier = "#"; idx += 1; }
            b'b' => { modifier = "b"; idx += 1; }
            b'-' => { modifier = "-"; idx += 1; }
            _ => {}
        }
    }
    if idx >= bytes.len() || !bytes[idx].is_ascii_digit() {
        return None;
    }
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx != bytes.len() {
        return None;
    }
    let octave = &token[1 + modifier.len()..];
    Some(format!("{}{}{}", first as char, modifier, octave))
}

fn try_parse_key_token(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    let bytes = lower.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if bytes[0] < b'a' || bytes[0] > b'g' {
        return None;
    }
    let mut idx = 1;
    if idx < bytes.len() && (bytes[idx] == b'#' || bytes[idx] == b'b') {
        idx += 1;
    }
    let quality = &lower[idx..];
    let canonical_quality = match quality {
        "maj" => "maj",
        "min" => "min",
        "m7" => "m7",
        "maj7" => "maj7",
        "min7" => "min7",
        "m7b5" => "m7b5",
        "dim" => "dim",
        "aug" => "aug",
        "sus2" => "sus2",
        "sus4" => "sus4",
        "add9" => "add9",
        "add11" => "add11",
        _ => return None,
    };
    let root = &lower[..idx];
    let first_upper = (root.as_bytes()[0] as char).to_ascii_uppercase();
    let rest = &root[1..];
    Some(format!("{first_upper}{rest}{canonical_quality}"))
}

const TEMPO_MARKINGS: &[&str] = &[
    "Largo", "Adagio", "Andante", "Moderato", "Allegro", "Presto", "Vivace", "Lento", "Grave",
];

fn try_parse_tempo_marking_token(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    for canonical in TEMPO_MARKINGS {
        if canonical.eq_ignore_ascii_case(&lower) {
            return Some((*canonical).to_string());
        }
    }
    None
}

fn try_parse_bpm_range_token(token: &str) -> Option<(u32, u32)> {
    let lower = token.to_lowercase();
    let stripped = lower.strip_suffix("bpm").unwrap_or(&lower);
    for sep in ['-', '_'] {
        if let Some(idx) = stripped.find(sep) {
            let (lo_str, hi_str) = stripped.split_at(idx);
            let hi_str = &hi_str[1..];
            if let (Ok(lo), Ok(hi)) = (lo_str.parse::<u32>(), hi_str.parse::<u32>()) {
                let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
                return Some((lo, hi));
            }
        }
    }
    None
}

fn try_parse_bpm_token(token: &str) -> Option<f32> {
    let lower = token.to_lowercase();
    if let Some(stripped) = lower.strip_suffix("bpm") {
        stripped.parse::<f32>().ok()
    } else {
        None
    }
}

fn is_standalone_bpm(token: &str) -> bool {
    if token.len() != 3 {
        return false;
    }
    token.bytes().all(|b| b.is_ascii_digit())
}

const KNOWN_TAGS: &[&str] = &[
    "kick", "snare", "rim", "clap", "hihat", "hat", "openhat", "cymbal", "crash", "ride",
    "tom", "perc", "percussion", "bass", "sub", "lead", "pad", "pluck", "stab",
    "vox", "vocal", "fx", "riser", "impact", "sweep", "glitch", "noise", "drone",
    "loop", "oneshot", "wet", "dry", "lofi", "mono", "stereo",
    "soft", "hard", "warm", "bright", "dark", "punchy",
];

fn try_parse_tag_token(token: &str) -> Option<String> {
    let lower = token.to_lowercase();
    if KNOWN_TAGS.contains(&lower.as_str()) {
        Some(lower)
    } else {
        None
    }
}

// ── ISO 8601 formatting ──

fn format_iso8601(unix_secs: u64) -> String {
    let secs = unix_secs as i64;
    let days = secs / 86400;
    let time_secs = (secs % 86400) as u32;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let mut year = 1970i64;
    let mut remaining = days;

    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md {
            month = i + 1;
            break;
        }
        remaining -= md;
    }
    if month == 0 {
        month = 12;
    }
    let day = remaining + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kick_c4() {
        let m = parse_filename("Kick_C4.wav");
        assert_eq!(m.tags, vec!["kick"]);
        assert_eq!(m.root_note.as_deref(), Some("C4"));
        assert_eq!(m.bpm, None);
    }

    #[test]
    fn test_parse_snare_rim_120bpm() {
        let m = parse_filename("Snare_Rim_120bpm.wav");
        assert_eq!(m.tags, vec!["snare", "rim"]);
        assert_eq!(m.root_note, None);
        assert!((m.bpm.unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_bass_c2_c4() {
        let m = parse_filename("Bass_C2_C4.wav");
        assert_eq!(m.tags, vec!["bass"]);
        assert_eq!(m.root_note.as_deref(), Some("C2"));
    }

    #[test]
    fn test_parse_no_match() {
        let m = parse_filename("vinyl_noise.wav");
        assert_eq!(m.category, None);
        assert_eq!(m.root_note, None);
        assert_eq!(m.bpm, None);
        assert!(m.tags.is_empty());
    }

    #[test]
    fn test_parse_pad_strings_a3() {
        let m = parse_filename("Pad_Strings_A3.wav");
        assert_eq!(m.category.as_deref(), Some("Strings"));
        assert_eq!(m.tags, vec!["pad"]);
        assert_eq!(m.root_note.as_deref(), Some("A3"));
    }

    #[test]
    fn test_parse_note_dsharp3() {
        let m = parse_filename("Kick_D#3.wav");
        assert_eq!(m.tags, vec!["kick"]);
        assert_eq!(m.root_note.as_deref(), Some("D#3"));
    }

    #[test]
    fn test_parse_note_flat() {
        let m = parse_filename("Bass_Bb2.wav");
        assert_eq!(m.tags, vec!["bass"]);
        assert_eq!(m.root_note.as_deref(), Some("Bb2"));
    }

    #[test]
    fn test_parse_standalone_bpm() {
        let m = parse_filename("Kick_120.wav");
        assert_eq!(m.tags, vec!["kick"]);
        assert_eq!(m.root_note, None);
        assert!((m.bpm.unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_note_with_dash() {
        let m = parse_filename("C-4_Piano.wav");
        assert_eq!(m.root_note.as_deref(), Some("C-4"));
        assert_eq!(m.category.as_deref(), Some("Piano"));
    }

    #[test]
    fn test_parse_key_maj() {
        let m = parse_filename("Cmaj.wav");
        assert_eq!(m.key.as_deref(), Some("Cmaj"));
        assert!(m.tags.is_empty());
    }

    #[test]
    fn test_parse_key_min() {
        let m = parse_filename("Amin_120bpm.wav");
        assert_eq!(m.key.as_deref(), Some("Amin"));
        assert!((m.bpm.unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_key_sharp() {
        let m = parse_filename("A#min.wav");
        assert_eq!(m.key.as_deref(), Some("A#min"));
    }

    #[test]
    fn test_parse_tags() {
        let m = parse_filename("Kick_punchy_120bpm.wav");
        assert!(m.tags.contains(&"kick".to_string()));
        assert!(m.tags.contains(&"punchy".to_string()));
        assert!((m.bpm.unwrap() - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_bpm_range() {
        let m = parse_filename("Loop_120-130bpm.wav");
        assert_eq!(m.bpm_range, Some((120, 130)));
    }

    #[test]
    fn test_parse_tempo_marking() {
        let m = parse_filename("Allegro_Pad_C4.wav");
        assert_eq!(m.tempo_marking.as_deref(), Some("Allegro"));
        assert_eq!(m.root_note.as_deref(), Some("C4"));
    }

    #[test]
    fn test_no_tags_without_musical_signal() {
        let m = parse_filename("Kick.wav");
        assert!(m.tags.is_empty());
        assert_eq!(m.category, None);
    }

    fn entry_with_bpm_key_tags_channels(
        bpm: Option<f32>,
        key: Option<&str>,
        tags: &[&str],
        channels: Option<u32>,
    ) -> LibraryEntry {
        LibraryEntry {
            name: String::from("test.wav"),
            path: String::from("/test/test.wav"),
            is_directory: false,
            duration: Some(1.0),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            file_size: 0,
            modified: String::new(),
            category: None,
            root_note: None,
            bpm,
            channels,
            key: key.map(String::from),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            bpm_range: None,
            tempo_marking: None,
        }
    }

    #[test]
    fn test_filter_by_bpm() {
        let entry = entry_with_bpm_key_tags_channels(Some(120.0), None, &[], Some(1));
        let f = DirFilter {
            min_bpm: Some(100.0),
            max_bpm: Some(140.0),
            ..Default::default()
        };
        assert!(f.matches(&entry));
        let f2 = DirFilter {
            min_bpm: Some(130.0),
            ..Default::default()
        };
        assert!(!f2.matches(&entry));
    }

    #[test]
    fn test_filter_by_key() {
        let entry = entry_with_bpm_key_tags_channels(None, Some("Cmaj"), &[], Some(1));
        let f = DirFilter {
            key: Some("Cmaj".to_string()),
            ..Default::default()
        };
        assert!(f.matches(&entry));
        let f2 = DirFilter {
            key: Some("Dmin".to_string()),
            ..Default::default()
        };
        assert!(!f2.matches(&entry));
    }

    #[test]
    fn test_filter_by_tag() {
        let entry = entry_with_bpm_key_tags_channels(None, None, &["kick", "punchy"], Some(1));
        let f = DirFilter {
            tag: Some("kick".to_string()),
            ..Default::default()
        };
        assert!(f.matches(&entry));
        let f2 = DirFilter {
            tag: Some("snare".to_string()),
            ..Default::default()
        };
        assert!(!f2.matches(&entry));
    }

    #[test]
    fn test_filter_by_channels() {
        let mono = entry_with_bpm_key_tags_channels(None, None, &[], Some(1));
        let stereo = entry_with_bpm_key_tags_channels(None, None, &[], Some(2));
        let f_mono = DirFilter {
            channels_filter: Some(1),
            ..Default::default()
        };
        assert!(f_mono.matches(&mono));
        assert!(!f_mono.matches(&stereo));
        let f_stereo = DirFilter {
            channels_filter: Some(2),
            ..Default::default()
        };
        assert!(!f_stereo.matches(&mono));
        assert!(f_stereo.matches(&stereo));
    }

    #[test]
    fn test_json_roundtrip() {
        let mut lib = SampleLibrary::new();
        lib.set_roots(vec![PathBuf::from("/samples")]);
        let entry_a = LibraryEntry {
            name: String::from("Kick_C4.wav"),
            path: String::from("/samples/Kick_C4.wav"),
            is_directory: false,
            duration: Some(0.5),
            sample_rate: Some(44100),
            bit_depth: Some(16),
            file_size: 44100,
            modified: String::from("2026-06-01T00:00:00Z"),
            category: None,
            root_note: Some(String::from("C4")),
            bpm: None,
            channels: Some(1),
            key: None,
            tags: vec![String::from("kick")],
            bpm_range: None,
            tempo_marking: None,
        };
        let entry_b = LibraryEntry {
            name: String::from("Cmaj_120bpm.wav"),
            path: String::from("/samples/Cmaj_120bpm.wav"),
            is_directory: false,
            duration: Some(2.0),
            sample_rate: Some(48000),
            bit_depth: Some(24),
            file_size: 288000,
            modified: String::from("2026-06-02T00:00:00Z"),
            category: None,
            root_note: None,
            bpm: Some(120.0),
            channels: Some(2),
            key: Some(String::from("Cmaj")),
            tags: vec![],
            bpm_range: Some((120, 130)),
            tempo_marking: Some(String::from("Allegro")),
        };
        lib.cache.insert(PathBuf::from("/samples/Kick_C4.wav"), entry_a);
        lib.cache.insert(PathBuf::from("/samples/Cmaj_120bpm.wav"), entry_b);

        let json = lib.to_json().expect("serialize");
        let lib2 = SampleLibrary::from_json(&json).expect("deserialize");

        assert_eq!(lib2.cache.len(), 2);
        assert_eq!(lib2.roots, vec![PathBuf::from("/samples")]);
        let e_a = lib2.cache.get(&PathBuf::from("/samples/Kick_C4.wav")).expect("entry a");
        assert_eq!(e_a.key, None);
        assert_eq!(e_a.tags, vec![String::from("kick")]);
        assert_eq!(e_a.root_note.as_deref(), Some("C4"));
        let e_b = lib2.cache.get(&PathBuf::from("/samples/Cmaj_120bpm.wav")).expect("entry b");
        assert_eq!(e_b.key.as_deref(), Some("Cmaj"));
        assert_eq!(e_b.bpm_range, Some((120, 130)));
        assert_eq!(e_b.tempo_marking.as_deref(), Some("Allegro"));
    }

    #[test]
    fn test_json_backcompat_without_new_fields() {
        // Old serialized data without the new key/tags/bpm_range/tempo_marking
        // fields must still deserialize (the #[serde(default)] on each field
        // gives them None / Vec::new() when missing).
        let old_json = r#"{
            "roots": ["/old"],
            "cache": {
                "/old/old.wav": {
                    "name": "old.wav",
                    "path": "/old/old.wav",
                    "is_directory": false,
                    "duration": null,
                    "sample_rate": null,
                    "bit_depth": null,
                    "file_size": 0,
                    "modified": "",
                    "category": null,
                    "root_note": null,
                    "bpm": null,
                    "channels": null
                }
            },
            "dir_cache": {}
        }"#;
        let lib = SampleLibrary::from_json(old_json).expect("deserialize old shape");
        assert_eq!(lib.cache.len(), 1);
        let entry = lib.cache.get(&PathBuf::from("/old/old.wav")).unwrap();
        assert_eq!(entry.key, None);
        assert!(entry.tags.is_empty());
        assert_eq!(entry.bpm_range, None);
        assert_eq!(entry.tempo_marking, None);
    }
}
