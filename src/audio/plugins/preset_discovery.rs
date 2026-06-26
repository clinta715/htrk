use std::collections::HashMap;
use std::ffi::CStr;
use std::path::{Path, PathBuf};

use clack_extensions::preset_discovery::factory::PresetDiscoveryFactory;
use clack_extensions::preset_discovery::indexer::IndexerImpl;
use clack_extensions::preset_discovery::metadata_receiver::MetadataReceiverImpl;
use clack_extensions::preset_discovery::preset_data::{
    FileType, Flags, Location, LocationInfo, Soundpack,
};
use clack_extensions::preset_discovery::provider::{Provider, ProviderInstanceError};
use clack_host::prelude::{HostInfo, PluginEntry};

use super::preset_library::PresetEntry;

#[derive(Debug)]
pub enum PresetScanError {
    LoadFailed(String),
    NoPresetDiscoveryFactory,
    ProviderError(ProviderInstanceError),
    ScanError(String),
}

impl std::fmt::Display for PresetScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetScanError::LoadFailed(s) => write!(f, "Load failed: {s}"),
            PresetScanError::NoPresetDiscoveryFactory => {
                write!(f, "Plugin does not expose preset-discovery factory")
            }
            PresetScanError::ProviderError(e) => write!(f, "Provider error: {e}"),
            PresetScanError::ScanError(s) => write!(f, "Scan error: {s}"),
        }
    }
}

impl std::error::Error for PresetScanError {}

// ── Indexer (collects filetypes and locations during provider init) ──

#[derive(Clone, Debug)]
struct LocationInfoOwned {
    name: String,
    flags: u32,
    kind: LocationKind,
}

#[derive(Clone, Debug)]
enum LocationKind {
    Plugin,
    File(String),
}

#[derive(Clone, Debug)]
struct FileTypeOwned {
    name: String,
    description: Option<String>,
    extension: Option<String>,
}

struct ScanIndexer {
    filetypes: Vec<FileTypeOwned>,
    locations: Vec<LocationInfoOwned>,
}

impl IndexerImpl for ScanIndexer {
    fn declare_filetype(&mut self, ft: FileType) -> Result<(), clack_host::prelude::HostError> {
        self.filetypes.push(FileTypeOwned {
            name: ft.name.to_string_lossy().into_owned(),
            description: ft.description.map(|s| s.to_string_lossy().into_owned()),
            extension: ft.file_extension.map(|s| s.to_string_lossy().into_owned()),
        });
        Ok(())
    }

    fn declare_location(&mut self, info: LocationInfo) -> Result<(), clack_host::prelude::HostError> {
        let kind = match info.location {
            Location::Plugin => LocationKind::Plugin,
            Location::File { path } => LocationKind::File(path.to_string_lossy().into_owned()),
        };
        self.locations.push(LocationInfoOwned {
            name: info.name.to_string_lossy().into_owned(),
            flags: info.flags.bits(),
            kind,
        });
        Ok(())
    }

    fn declare_soundpack(&mut self, _sp: Soundpack) -> Result<(), clack_host::prelude::HostError> {
        Ok(())
    }
}

// ── Receiver (collects preset metadata from get_metadata calls) ──

struct ScanReceiver {
    entries: Vec<PresetEntry>,
    // Context
    plugin_path: String,
    plugin_id: String,
    plugin_name: String,
    provider_id: String,
    provider_name: String,
    // Accumulator for current preset
    cur_name: Option<String>,
    cur_load_key: Option<String>,
    cur_flags: u32,
    cur_features: Vec<String>,
    cur_creators: Vec<String>,
    cur_description: Option<String>,
    cur_plugin_ids: Vec<String>,
    cur_soundpack_id: Option<String>,
    cur_extra_info: HashMap<String, String>,
    cur_creation_time: Option<i64>,
    cur_modification_time: Option<i64>,
}

impl ScanReceiver {
    fn new(
        plugin_path: String,
        plugin_id: String,
        plugin_name: String,
        provider_id: String,
        provider_name: String,
    ) -> Self {
        ScanReceiver {
            entries: Vec::new(),
            plugin_path,
            plugin_id,
            plugin_name,
            provider_id,
            provider_name,
            cur_name: None,
            cur_load_key: None,
            cur_flags: 0,
            cur_features: Vec::new(),
            cur_creators: Vec::new(),
            cur_description: None,
            cur_plugin_ids: Vec::new(),
            cur_soundpack_id: None,
            cur_extra_info: HashMap::new(),
            cur_creation_time: None,
            cur_modification_time: None,
        }
    }

    fn finalize(&mut self, location_path: Option<String>, location_kind: &str) {
        let name = match self.cur_name.take() {
            Some(n) => n,
            None => return,
        };
        let load_key = self.cur_load_key.take().unwrap_or_default();
        self.entries.push(PresetEntry {
            plugin_path: self.plugin_path.clone(),
            plugin_id: self.plugin_id.clone(),
            plugin_name: self.plugin_name.clone(),
            provider_id: self.provider_id.clone(),
            provider_name: self.provider_name.clone(),
            name,
            load_key,
            location_path,
            location_kind: location_kind.to_string(),
            flags: self.cur_flags,
            features: std::mem::take(&mut self.cur_features),
            description: self.cur_description.take(),
            creators: std::mem::take(&mut self.cur_creators),
            compatible_plugin_ids: std::mem::take(&mut self.cur_plugin_ids),
            soundpack_id: self.cur_soundpack_id.take(),
            extra_info: std::mem::take(&mut self.cur_extra_info),
            creation_time: self.cur_creation_time.take(),
            modification_time: self.cur_modification_time.take(),
        });
    }
}

impl MetadataReceiverImpl for ScanReceiver {
    fn on_error(&mut self, _code: i32, msg: Option<&CStr>) {
        eprintln!(
            "[preset_discovery] Metadata error: {}",
            msg.map(|s| s.to_string_lossy()).unwrap_or_default()
        );
    }

    fn begin_preset(
        &mut self,
        name: Option<&CStr>,
        load_key: Option<&CStr>,
    ) -> Result<(), clack_host::prelude::HostError> {
        self.finalize(None, "");
        self.cur_name = name.map(|s| s.to_string_lossy().into_owned());
        self.cur_load_key = load_key.map(|s| s.to_string_lossy().into_owned());
        Ok(())
    }

    fn add_plugin_id(&mut self, pid: clack_common::utils::UniversalPluginId) {
        self.cur_plugin_ids.push(pid.id.to_string_lossy().into_owned());
    }

    fn set_soundpack_id(&mut self, id: &CStr) {
        self.cur_soundpack_id = Some(id.to_string_lossy().into_owned());
    }

    fn set_flags(&mut self, flags: Flags) {
        self.cur_flags = flags.bits();
    }

    fn add_creator(&mut self, creator: &CStr) {
        self.cur_creators.push(creator.to_string_lossy().into_owned());
    }

    fn set_description(&mut self, desc: &CStr) {
        self.cur_description = Some(desc.to_string_lossy().into_owned());
    }

    fn set_timestamps(
        &mut self,
        creation: Option<clack_common::utils::Timestamp>,
        modified: Option<clack_common::utils::Timestamp>,
    ) {
        self.cur_creation_time = creation.map(|t| t.seconds_since_epoch() as i64);
        self.cur_modification_time = modified.map(|t| t.seconds_since_epoch() as i64);
    }

    fn add_feature(&mut self, feature: &CStr) {
        self.cur_features.push(feature.to_string_lossy().into_owned());
    }

    fn add_extra_info(&mut self, key: &CStr, value: &CStr) {
        self.cur_extra_info.insert(
            key.to_string_lossy().into_owned(),
            value.to_string_lossy().into_owned(),
        );
    }
}

// ── Scanning ──

/// Scan a single .clap plugin for presets.
/// Returns `Ok(entries)` or an error explaining why scanning failed.
pub fn scan_plugin_presets(path: &Path) -> Result<Vec<PresetEntry>, PresetScanError> {
    let load_path = resolve_load_path(path)?;
    let entry = unsafe {
        PluginEntry::load(&load_path)
            .map_err(|e| PresetScanError::LoadFailed(e.to_string()))?
    };

    let plugin_factory = entry
        .get_plugin_factory()
        .ok_or_else(|| PresetScanError::LoadFailed("No plugin factory".into()))?;

    let clap_desc = plugin_factory
        .plugin_descriptors()
        .next()
        .ok_or_else(|| PresetScanError::LoadFailed("No plugins in bundle".into()))?;

    let plugin_id = clap_desc
        .id()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plugin_name = clap_desc
        .name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".into());

    let factory: PresetDiscoveryFactory = entry
        .get_factory()
        .ok_or(PresetScanError::NoPresetDiscoveryFactory)?;

    let host_info = HostInfo::new(
        "htrk",
        "htrk",
        "https://github.com/clinta715/htrk",
        env!("CARGO_PKG_VERSION"),
    )
    .map_err(|e| PresetScanError::LoadFailed(e.to_string()))?;

    if factory.provider_count() == 0 {
        return Ok(Vec::new());
    }

    let mut all_presets = Vec::new();

    for provider_desc in factory.provider_descriptors() {
        let pid_cstr = match provider_desc.id() {
            Some(id) => id,
            None => continue,
        };
        let pid = pid_cstr.to_string_lossy().into_owned();
        let pname = provider_desc
            .name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let indexer = ScanIndexer {
            filetypes: Vec::new(),
            locations: Vec::new(),
        };

        let mut provider = match Provider::instantiate(indexer, &entry, pid_cstr, &host_info) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "[preset_discovery] Provider '{pid}' for {plugin_name} failed: {e:?}"
                );
                continue;
            }
        };

        let filetypes = provider.indexer().filetypes.clone();
        let locations = provider.indexer().locations.clone();

        if locations.is_empty() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        for loc in &locations {
            let mut receiver = ScanReceiver::new(
                path_str.clone(),
                plugin_id.clone(),
                plugin_name.clone(),
                pid.clone(),
                pname.clone(),
            );

            match &loc.kind {
                LocationKind::Plugin => {
                    provider.get_metadata(Location::Plugin, &mut receiver);
                    receiver.finalize(None, "plugin");
                }
                LocationKind::File(loc_path) => {
                    let dir = PathBuf::from(loc_path);
                    if dir.is_dir() {
                        let matching = find_matching_files(&dir, &filetypes);
                        for file_path in &matching {
                            // CString for FFI; lives on stack for the call
                            let fp = file_path.to_string_lossy();
                            let Ok(cpath) = std::ffi::CString::new(fp.as_ref()) else {
                                continue;
                            };
                            provider.get_metadata(
                                Location::File {
                                    path: cpath.as_c_str(),
                                },
                                &mut receiver,
                            );
                            receiver.finalize(
                                Some(file_path.to_string_lossy().to_string()),
                                "file",
                            );
                        }
                    } else if dir.is_file() || dir.exists() {
                        let fp = dir.to_string_lossy();
                        if let Ok(cpath) = std::ffi::CString::new(fp.as_ref()) {
                            provider.get_metadata(
                                Location::File {
                                    path: cpath.as_c_str(),
                                },
                                &mut receiver,
                            );
                            receiver.finalize(Some(dir.to_string_lossy().to_string()), "file");
                        }
                    }
                }
            }

            all_presets.extend(receiver.entries);
        }
    }

    Ok(all_presets)
}

/// Scan each plugin path for presets. Resilient: per-plugin failures are
/// collected as (path, error) tuples, not propagated.
pub fn scan_plugins_for_presets(
    plugin_paths: &[PathBuf],
) -> (Vec<PresetEntry>, Vec<(PathBuf, String)>) {
    let mut all = Vec::new();
    let mut errors = Vec::new();

    for path in plugin_paths {
        match scan_plugin_presets(path) {
            Ok(entries) => {
                if !entries.is_empty() {
                    eprintln!(
                        "[preset_discovery] {} preset(s) from {}",
                        entries.len(),
                        path.display()
                    );
                }
                all.extend(entries);
            }
            Err(PresetScanError::NoPresetDiscoveryFactory) => {}
            Err(e) => {
                eprintln!("[preset_discovery] Failed to scan {}: {e}", path.display());
                errors.push((path.clone(), e.to_string()));
            }
        }
    }

    (all, errors)
}

// ── Helpers ──

fn resolve_load_path(path: &Path) -> Result<PathBuf, PresetScanError> {
    if path.is_dir() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| PresetScanError::LoadFailed("Bundle missing name".into()))?;
        let dll = path.join(format!("{stem}.clap"));
        if !dll.is_file() {
            return Err(PresetScanError::LoadFailed(format!(
                "Bundle {} has no {} DLL",
                path.display(),
                dll.display()
            )));
        }
        Ok(dll)
    } else {
        Ok(path.to_path_buf())
    }
}

fn find_matching_files(
    dir: &Path,
    filetypes: &[FileTypeOwned],
) -> Vec<PathBuf> {
    let mut results = Vec::new();
    walk_dir(dir, &mut results, filetypes);
    results
}

fn walk_dir(dir: &Path, results: &mut Vec<PathBuf>, filetypes: &[FileTypeOwned]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, results, filetypes);
        } else if path.is_file() {
            if filetypes.is_empty() || matches_filetype(&path, filetypes) {
                results.push(path);
            }
        }
    }
}

fn matches_filetype(path: &Path, filetypes: &[FileTypeOwned]) -> bool {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    filetypes.iter().any(|ft| {
        ft.extension.as_deref().map_or(true, |fe| fe.to_lowercase() == ext)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_load_path_direct_file() {
        let p = Path::new("/some/path/plugin.clap");
        assert_eq!(resolve_load_path(p).unwrap(), p);
    }

    /// Scan every .clap plugin in the system CLAP directory and report
    /// which ones expose the preset-discovery factory and how many presets.
    #[test]
    fn test_scan_all_clap_presets() {
        let root = Path::new(r"C:\Program Files\Common Files\CLAP");
        if !root.is_dir() {
            eprintln!("[skip] CLAP directory not found");
            return;
        }

        let mut total = 0usize;
        let mut with_presets = 0usize;
        let mut without_factory = 0usize;
        let mut errors = 0usize;

        for entry in std::fs::read_dir(root).unwrap() {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("clap") {
                continue;
            }
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");

            match scan_plugin_presets(&path) {
                Ok(entries) => {
                    total += 1;
                    eprintln!("  ✓ {}: {} preset(s)", name, entries.len());
                    if !entries.is_empty() {
                        with_presets += 1;
                        for (i, e) in entries.iter().enumerate().take(3) {
                            eprintln!("      {}. {} [{}]", i + 1, e.name, e.features.join(", "));
                        }
                        if entries.len() > 3 {
                            eprintln!("      ... and {} more", entries.len() - 3);
                        }
                    }
                }
                Err(PresetScanError::NoPresetDiscoveryFactory) => {
                    total += 1;
                    without_factory += 1;
                    eprintln!("  ~ {}: no preset-discovery factory", name);
                }
                Err(e) => {
                    total += 1;
                    errors += 1;
                    eprintln!("  ✗ {}: {e}", name);
                }
            }
        }

        eprintln!(
            "\n[result] {total} plugin(s): {with_presets} with presets, \
             {without_factory} no factory, {errors} error(s)"
        );
    }
}
