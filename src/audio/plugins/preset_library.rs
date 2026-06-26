use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// A single cached preset entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PresetEntry {
    // Plugin identity
    pub plugin_path: String,
    pub plugin_id: String,
    pub plugin_name: String,
    // Provider identity
    pub provider_id: String,
    pub provider_name: String,
    // Preset identity
    pub name: String,
    pub load_key: String,
    pub location_path: Option<String>,
    pub location_kind: String,
    // Metadata
    pub flags: u32,
    pub features: Vec<String>,
    pub description: Option<String>,
    pub creators: Vec<String>,
    pub compatible_plugin_ids: Vec<String>,
    pub soundpack_id: Option<String>,
    pub extra_info: HashMap<String, String>,
    // Timestamps (unix seconds)
    pub creation_time: Option<i64>,
    pub modification_time: Option<i64>,
}

impl PresetEntry {
    /// Stable key used for deduplication in the library cache.
    pub fn cache_key(&self) -> String {
        let loc = self.location_path.as_deref().unwrap_or("plugin");
        format!("{}:{}:{}:{}:{}", self.plugin_path, self.provider_id, loc, self.plugin_id, self.load_key)
    }

    fn plugin_key(&self) -> String {
        format!("clap:{}:{}", self.plugin_path, self.plugin_id)
    }
}

/// Per-session in-memory cache of discovered plugin presets.
///
/// Mirrors the PluginLibrary and SampleLibrary patterns:
/// - `presets`: unique presets keyed by `cache_key()`
/// - `plugin_presets`: maps `plugin_key -> Vec<cache_key>` for per-plugin lookups
///
/// Supports search, pagination, and JSON persistence.
pub struct PresetLibrary {
    presets: HashMap<String, PresetEntry>,
    plugin_presets: HashMap<String, Vec<String>>,
    last_scan_time: Option<SystemTime>,
}

#[derive(Serialize, Deserialize)]
struct PresetLibraryData {
    presets: Vec<PresetEntry>,
    last_scan_time_secs: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PresetSearchResults {
    pub total_results: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    pub results: Vec<PresetEntry>,
}

impl PresetLibrary {
    pub fn new() -> Self {
        PresetLibrary {
            presets: HashMap::new(),
            plugin_presets: HashMap::new(),
            last_scan_time: None,
        }
    }

    pub fn add_preset(&mut self, entry: PresetEntry) {
        let key = entry.cache_key();
        let plugin_key = entry.plugin_key();
        if !self.presets.contains_key(&key) {
            self.plugin_presets
                .entry(plugin_key)
                .or_default()
                .push(key.clone());
        }
        self.presets.insert(key, entry);
    }

    pub fn add_presets(&mut self, entries: Vec<PresetEntry>) {
        for entry in entries {
            self.add_preset(entry);
        }
    }

    pub fn get_preset(&self, key: &str) -> Option<&PresetEntry> {
        self.presets.get(key)
    }

    pub fn list_presets(&self) -> Vec<&PresetEntry> {
        self.presets.values().collect()
    }

    pub fn preset_count(&self) -> usize {
        self.presets.len()
    }

    /// List presets for a specific plugin (by `plugin_path:plugin_id`).
    pub fn list_plugin_presets(&self, plugin_path: &str, plugin_id: &str) -> Vec<&PresetEntry> {
        let pkey = format!("clap:{plugin_path}:{plugin_id}");
        self.plugin_presets
            .get(&pkey)
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| self.presets.get(k))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Search presets by name, feature, or plugin_name substring.
    /// Supports pagination.
    pub fn search(
        &self,
        query: &str,
        filter_plugin_id: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> PresetSearchResults {
        let q = query.to_lowercase();
        let mut results: Vec<&PresetEntry> = self
            .presets
            .values()
            .filter(|e| {
                if let Some(pid) = filter_plugin_id {
                    if e.plugin_id != pid {
                        return false;
                    }
                }
                if q.is_empty() {
                    return true;
                }
                e.name.to_lowercase().contains(&q)
                    || e.plugin_name.to_lowercase().contains(&q)
                    || e.features.iter().any(|f| f.to_lowercase().contains(&q))
                    || e.description.as_ref().map_or(false, |d| d.to_lowercase().contains(&q))
                    || e.creators.iter().any(|c| c.to_lowercase().contains(&q))
                    || e.extra_info.values().any(|v| v.to_lowercase().contains(&q))
            })
            .collect();

        results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let total_results = results.len();
        let total_pages = if page_size == 0 {
            1
        } else {
            (total_results + page_size - 1) / page_size
        };
        let start = page * page_size;
        let page_results = results
            .into_iter()
            .skip(start)
            .take(page_size)
            .cloned()
            .collect();

        PresetSearchResults {
            total_results,
            page,
            page_size,
            total_pages,
            results: page_results,
        }
    }

    pub fn clear(&mut self) {
        self.presets.clear();
        self.plugin_presets.clear();
        self.last_scan_time = None;
    }

    pub fn last_scan_time(&self) -> Option<SystemTime> {
        self.last_scan_time
    }

    pub fn set_last_scan_time(&mut self, time: SystemTime) {
        self.last_scan_time = Some(time);
    }

    /// Serialize the library to a JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        let data = PresetLibraryData {
            presets: self.presets.values().cloned().collect(),
            last_scan_time_secs: self
                .last_scan_time
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        };
        serde_json::to_string_pretty(&data).map_err(|e| e.to_string())
    }

    /// Deserialize the library from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let data: PresetLibraryData =
            serde_json::from_str(json).map_err(|e| format!("Invalid preset cache: {e}"))?;
        let mut lib = PresetLibrary::new();
        lib.add_presets(data.presets);
        lib.last_scan_time = data
            .last_scan_time_secs
            .map(|secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs));
        Ok(lib)
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
            .map_err(|e| format!("Cannot write preset cache: {e}"))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("Cannot rename preset cache: {e}"))?;
        Ok(())
    }

    /// Load the library from a JSON file at `path`.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read preset cache: {e}"))?;
        Self::from_json(&json)
    }
}

impl Default for PresetLibrary {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(name: &str, plugin: &str, features: &[&str]) -> PresetEntry {
        PresetEntry {
            plugin_path: format!("/plugins/{plugin}.clap"),
            plugin_id: format!("com.test.{plugin}"),
            plugin_name: plugin.to_string(),
            provider_id: "main".into(),
            provider_name: plugin.to_string(),
            name: name.to_string(),
            load_key: name.to_string(),
            location_path: Some("/presets".into()),
            location_kind: "file".into(),
            flags: 0,
            features: features.iter().map(|s| s.to_string()).collect(),
            description: None,
            creators: vec!["Test Author".into()],
            compatible_plugin_ids: vec![],
            soundpack_id: None,
            extra_info: HashMap::new(),
            creation_time: None,
            modification_time: None,
        }
    }

    #[test]
    fn test_empty_library() {
        let lib = PresetLibrary::new();
        assert_eq!(lib.preset_count(), 0);
        let results = lib.search("", None, 0, 10);
        assert_eq!(results.total_results, 0);
    }

    #[test]
    fn test_add_and_get() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Pad Warm", "Synth1", &["pad", "warm"]));
        assert_eq!(lib.preset_count(), 1);
        let p = lib.list_presets();
        assert_eq!(p[0].name, "Pad Warm");
    }

    #[test]
    fn test_dedup_by_cache_key() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Pad Warm", "Synth1", &["pad"]));
        lib.add_preset(sample_entry("Pad Warm", "Synth1", &["pad"]));
        assert_eq!(lib.preset_count(), 1);
    }

    #[test]
    fn test_search_by_name() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Pad Warm", "Synth1", &["pad"]));
        lib.add_preset(sample_entry("Bass Sub", "Synth1", &["bass"]));
        lib.add_preset(sample_entry("Lead Bright", "Synth2", &["lead"]));
        let results = lib.search("bass", None, 0, 10);
        assert_eq!(results.total_results, 1);
        assert_eq!(results.results[0].name, "Bass Sub");
    }

    #[test]
    fn test_search_by_feature() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Warmth", "Synth1", &["pad", "atmospheric"]));
        lib.add_preset(sample_entry("Sub Drop", "Synth1", &["bass", "sub"]));
        let results = lib.search("atmospheric", None, 0, 10);
        assert_eq!(results.total_results, 1);
        assert_eq!(results.results[0].name, "Warmth");
    }

    #[test]
    fn test_search_pagination() {
        let mut lib = PresetLibrary::new();
        for i in 0..25 {
            lib.add_preset(sample_entry(&format!("Preset {i:02}"), "Synth1", &[]));
        }
        let page1 = lib.search("", None, 0, 10);
        assert_eq!(page1.total_results, 25);
        assert_eq!(page1.results.len(), 10);
        assert_eq!(page1.total_pages, 3);

        let page2 = lib.search("", None, 1, 10);
        assert_eq!(page2.results.len(), 10);

        let page3 = lib.search("", None, 2, 10);
        assert_eq!(page3.results.len(), 5);
    }

    #[test]
    fn test_filter_by_plugin_id() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Pad A", "Synth1", &[]));
        lib.add_preset(sample_entry("Pad B", "Synth2", &[]));
        let results = lib.search("", Some("com.test.Synth1"), 0, 10);
        assert_eq!(results.total_results, 1);
        assert_eq!(results.results[0].name, "Pad A");
    }

    #[test]
    fn test_list_plugin_presets() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("A", "Synth1", &[]));
        lib.add_preset(sample_entry("B", "Synth1", &[]));
        lib.add_preset(sample_entry("C", "Synth2", &[]));
        let p1 = lib.list_plugin_presets("/plugins/Synth1.clap", "com.test.Synth1");
        assert_eq!(p1.len(), 2);
        let p2 = lib.list_plugin_presets("/plugins/Synth2.clap", "com.test.Synth2");
        assert_eq!(p2.len(), 1);
    }

    #[test]
    fn test_json_roundtrip() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("Pad", "Synth1", &["warm"]));
        lib.add_preset(sample_entry("Bass", "Synth1", &["deep"]));
        lib.set_last_scan_time(std::time::UNIX_EPOCH);
        let json = lib.to_json().unwrap();
        let lib2 = PresetLibrary::from_json(&json).unwrap();
        assert_eq!(lib2.preset_count(), 2);
        assert!(lib2.last_scan_time().is_some());
        let results = lib2.search("Pad", None, 0, 10);
        assert_eq!(results.total_results, 1);
    }

    #[test]
    fn test_clear() {
        let mut lib = PresetLibrary::new();
        lib.add_preset(sample_entry("A", "Synth1", &[]));
        assert_eq!(lib.preset_count(), 1);
        lib.clear();
        assert_eq!(lib.preset_count(), 0);
    }
}
