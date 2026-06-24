// Plugin library — in-memory metadata cache for discovered plugins.
// Mirrors the SampleLibrary pattern: per-session, populated by scanning,
// read by MCP tools and the UI plugin browser.

use std::collections::HashMap;
use std::path::PathBuf;

use super::discovery::scan_paths;
use super::PluginDescriptor;

/// Per-session in-memory cache of discovered plugins.
/// Phase 1: stores scan results (file lists) and descriptors (once probed).
/// Not persistent; repopulated on each scan.
pub struct PluginLibrary {
    pub scan_roots: Vec<PathBuf>,
    descriptors: HashMap<String, PluginDescriptor>,
    last_scan_paths: Vec<PathBuf>,
}

impl PluginLibrary {
    pub fn new() -> Self {
        PluginLibrary {
            scan_roots: Vec::new(),
            descriptors: HashMap::new(),
            last_scan_paths: Vec::new(),
        }
    }

    /// Set scan roots and clear the cache.
    pub fn set_scan_roots(&mut self, roots: Vec<PathBuf>) {
        self.scan_roots = roots;
        self.descriptors.clear();
        self.last_scan_paths.clear();
    }

    /// Scan all configured roots for `.clap` files.
    /// Returns the list of paths found.
    pub fn scan(&mut self) -> Vec<PathBuf> {
        let found = scan_paths(&self.scan_roots).clap_files;
        self.last_scan_paths = found.clone();
        found
    }

    /// Add a descriptor to the cache (keyed by a stable id like `format:path:plugin_id`).
    pub fn add_descriptor(&mut self, descriptor: PluginDescriptor) {
        let key = descriptor_key(&descriptor);
        self.descriptors.insert(key, descriptor);
    }

    /// Look up a cached descriptor by (format, path, plugin_id).
    pub fn get_descriptor(
        &self,
        format: super::PluginFormat,
        path: &std::path::Path,
        plugin_id: &str,
    ) -> Option<&PluginDescriptor> {
        let key = make_key(format, path, plugin_id);
        self.descriptors.get(&key)
    }

    /// List all cached descriptors.
    pub fn list_descriptors(&self) -> Vec<&PluginDescriptor> {
        self.descriptors.values().collect()
    }

    /// Number of cached descriptors.
    pub fn descriptor_count(&self) -> usize {
        self.descriptors.len()
    }

    /// Last scan's file list.
    pub fn last_scan_paths(&self) -> &[PathBuf] {
        &self.last_scan_paths
    }

    /// Clear the descriptor cache (keeps scan roots and last scan paths).
    pub fn clear_cache(&mut self) {
        self.descriptors.clear();
    }
}

fn descriptor_key(d: &PluginDescriptor) -> String {
    make_key(d.format, &d.path, &d.plugin_id)
}

fn make_key(format: super::PluginFormat, path: &std::path::Path, plugin_id: &str) -> String {
    format!("{}:{}:{}", format.as_str(), path.display(), plugin_id)
}

impl Default for PluginLibrary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::plugins::PluginFormat;
    use crate::audio::plugins::PluginType;

    fn test_descriptor(name: &str, path: &str) -> PluginDescriptor {
        PluginDescriptor {
            format: PluginFormat::Clap,
            path: PathBuf::from(path),
            plugin_id: format!("test.{name}"),
            name: name.to_string(),
            vendor: "Test".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            plugin_type: PluginType::Effect,
            audio_inputs: 2,
            audio_outputs: 2,
            has_editor: false,
            supports_state: true,
        }
    }

    #[test]
    fn test_new_library() {
        let lib = PluginLibrary::new();
        assert_eq!(lib.descriptor_count(), 0);
        assert!(lib.scan_roots.is_empty());
    }

    #[test]
    fn test_add_and_get_descriptor() {
        let mut lib = PluginLibrary::new();
        let d = test_descriptor("Reverb", "/usr/lib/clap/reverb.clap");
        lib.add_descriptor(d.clone());
        assert_eq!(lib.descriptor_count(), 1);

        let got = lib.get_descriptor(PluginFormat::Clap, std::path::Path::new("/usr/lib/clap/reverb.clap"), "test.Reverb");
        assert!(got.is_some());
        assert_eq!(got.unwrap().name, "Reverb");
    }

    #[test]
    fn test_set_scan_roots_clears_cache() {
        let mut lib = PluginLibrary::new();
        lib.add_descriptor(test_descriptor("Reverb", "/a.clap"));
        assert_eq!(lib.descriptor_count(), 1);
        lib.set_scan_roots(vec![PathBuf::from("/some/path")]);
        assert_eq!(lib.descriptor_count(), 0);
    }

    #[test]
    fn test_list_descriptors() {
        let mut lib = PluginLibrary::new();
        lib.add_descriptor(test_descriptor("Reverb", "/a.clap"));
        lib.add_descriptor(test_descriptor("Delay", "/b.clap"));
        assert_eq!(lib.list_descriptors().len(), 2);
    }

    #[test]
    fn test_clear_cache() {
        let mut lib = PluginLibrary::new();
        lib.add_descriptor(test_descriptor("Reverb", "/a.clap"));
        lib.set_scan_roots(vec![PathBuf::from("/roots")]);
        lib.add_descriptor(test_descriptor("Delay", "/b.clap"));
        assert_eq!(lib.descriptor_count(), 1);
        lib.clear_cache();
        assert_eq!(lib.descriptor_count(), 0);
    }
}
