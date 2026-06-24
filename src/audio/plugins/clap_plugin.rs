// CLAP plugin loader (Phase 2).
//
// Skeleton implementation: provides the lifecycle scaffolding (load, activate,
// stop, deactivate) and a stub process() that passes audio through unchanged.
// The real CLAP processing pipeline (events, parameter queue, AudioPorts) is
// filled in incrementally as we integrate with real CLAP plugins.

use std::any::Any;
use std::path::Path;

use clack_host::prelude::*;
use clack_common::plugin::PluginDescriptor as ClapPluginDescriptor;

use super::{
    HostedPluginHandle, HostedPluginProcessor, ParamInfo, PluginDescriptor, PluginError,
    PluginFormat, PluginType, TransportInfo,
};

// ── Host Handler ──
//
// `()` works for all three handler types — clack provides default no-op impls.

pub struct HtrkHost;
impl HostHandlers for HtrkHost {
    type Shared<'a> = ();
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}

// ── Main-thread Handle ──

pub struct ClapPluginHandle {
    instance: Option<PluginInstance<HtrkHost>>,
    descriptor: PluginDescriptor,
    activated: bool,
}

impl ClapPluginHandle {
    /// Load a CLAP plugin from disk and instantiate it. Discovers the first plugin in the bundle.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        let entry = unsafe {
            PluginEntry::load(path).map_err(|e| PluginError::LoadFailed(e.to_string()))?
        };

        let plugin_factory = entry
            .get_plugin_factory()
            .ok_or(PluginError::LoadFailed("No plugin factory".into()))?;

        let clap_descriptor = plugin_factory
            .plugin_descriptors()
            .next()
            .ok_or(PluginError::LoadFailed("No plugins in bundle".into()))?;

        let host_info = HostInfo::new("htrk", "htrk", "https://github.com/clinta715/htrk", env!("CARGO_PKG_VERSION"))
            .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let plugin_id = clap_descriptor
            .id()
            .ok_or(PluginError::LoadFailed("Plugin missing id".into()))?
            .to_owned();

        let instance = PluginInstance::<HtrkHost>::new(
            |_| (),
            |_| (),
            &entry,
            &plugin_id,
            &host_info,
        )
        .map_err(|e| PluginError::LoadFailed(e.to_string()))?;

        let descriptor = extract_descriptor(path, &clap_descriptor);

        Ok(ClapPluginHandle {
            instance: Some(instance),
            descriptor,
            activated: false,
        })
    }

    /// Returns the descriptor.
    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }
}

fn extract_descriptor(path: &Path, clap_desc: &ClapPluginDescriptor) -> PluginDescriptor {
    let plugin_id = clap_desc
        .id()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let name = clap_desc
        .name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let vendor = clap_desc
        .vendor()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown".to_string());

    let description = clap_desc
        .description()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Determine plugin type heuristically from features
    let features: Vec<String> = clap_desc
        .features()
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();

    let plugin_type = if features.iter().any(|f| f == "instrument") {
        PluginType::Instrument
    } else if features.iter().any(|f| f.contains("effect") || f == "analyzer") {
        PluginType::Effect
    } else {
        PluginType::Both
    };

    PluginDescriptor {
        format: PluginFormat::Clap,
        path: path.to_path_buf(),
        plugin_id,
        name,
        vendor,
        version: String::new(),
        description,
        plugin_type,
        audio_inputs: 2,
        audio_outputs: 2,
        has_editor: false,
        supports_state: true,
    }
}

impl HostedPluginHandle for ClapPluginHandle {
    fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    fn activate(&mut self, sample_rate: f64, max_block: u32) -> Result<Box<dyn HostedPluginProcessor>, String> {
        let instance = self.instance.as_mut().ok_or_else(|| "Plugin not loaded".to_string())?;

        let config = PluginAudioConfiguration {
            sample_rate,
            min_frames_count: 1,
            max_frames_count: max_block.max(1),
        };

        instance
            .activate(|_, _| (), config)
            .map_err(|e| e.to_string())?;

        self.activated = true;
        Ok(Box::new(ClapPluginProcessor {
            descriptor: self.descriptor.clone(),
            sample_rate,
        }))
    }

    fn deactivate(&mut self, _stopped: Box<dyn Any>) -> Result<(), String> {
        if let Some(instance) = self.instance.as_mut() {
            if self.activated {
                // The `_stopped` Box carries the audio thread's StoppedPluginAudioProcessor
                // reference. When dropped, it releases the reference. We then call
                // try_deactivate_with to deallocate the audio processor.
                drop(_stopped);
                instance
                    .try_deactivate_with(|_audio_proc, _main_thread| ())
                    .map_err(|e| e.to_string())?;
                self.activated = false;
            }
        }
        Ok(())
    }

    fn save_state(&self) -> Result<Vec<u8>, String> {
        // State save requires the state extension and a live instance. Stub.
        Ok(Vec::new())
    }

    fn load_state(&mut self, _state: &[u8]) -> Result<(), String> {
        // State restore requires the state extension. Stub.
        Ok(())
    }

    fn parameter_info(&self) -> Vec<ParamInfo> {
        // Parameter enumeration requires the params extension. Stub.
        Vec::new()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// ── Audio-thread Processor (stub) ──

/// Stub processor that passes audio through. The real implementation will
/// wrap a `StartedPluginAudioProcessor<HtrkHost>` and call its process() method.
pub struct ClapPluginProcessor {
    descriptor: PluginDescriptor,
    sample_rate: f64,
}

impl HostedPluginProcessor for ClapPluginProcessor {
    fn process(
        &mut self,
        input_l: &[f32],
        input_r: &[f32],
        output_l: &mut [f32],
        output_r: &mut [f32],
        frame_count: usize,
        _transport: &TransportInfo,
    ) {
        // Pass-through (Phase 2 stub — real CLAP process() integration is filled in
        // incrementally as we test against real plugins).
        let n = frame_count.min(input_l.len()).min(output_l.len());
        output_l[..n].copy_from_slice(&input_l[..n]);
        output_r[..n].copy_from_slice(&input_r[..n]);
    }

    fn set_parameter(&mut self, _param_id: u32, _value: f32) {}
    fn get_parameter(&self, _param_id: u32) -> f32 { 0.0 }
    fn parameter_count(&self) -> u32 { 0 }
    fn latency(&self) -> u32 { 0 }
    fn name(&self) -> &str { &self.descriptor.name }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descriptor_extraction_basic() {
        let desc = PluginDescriptor {
            format: PluginFormat::Clap,
            path: std::path::PathBuf::from("/test/plugin.clap"),
            plugin_id: "test.id".into(),
            name: "Test Plugin".into(),
            vendor: "Tester".into(),
            version: "1.0".into(),
            description: "A test plugin".into(),
            plugin_type: PluginType::Effect,
            audio_inputs: 2,
            audio_outputs: 2,
            has_editor: false,
            supports_state: true,
        };
        assert_eq!(desc.name, "Test Plugin");
        assert_eq!(desc.format, PluginFormat::Clap);
    }

    #[test]
    fn test_invalid_path_returns_error() {
        let result = ClapPluginHandle::load(std::path::Path::new("/nonexistent/plugin.clap"));
        assert!(result.is_err());
    }
}
