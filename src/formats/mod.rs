pub mod common;
pub mod c669;
pub mod htk;
pub mod hti;
pub mod it;
pub mod midi;
pub mod mmd;
pub mod s3m;
pub mod stm;
pub mod ult;
pub mod wav;
pub mod xm;
pub mod modfile;

use crate::errors::FormatResult;
use crate::sequencer::instrument::Instrument;
use crate::sequencer::sample::Sample;
use crate::sequencer::{Module, ModuleFormat};

#[allow(dead_code)]
pub trait FormatHandler {
    fn format_id(&self) -> &'static str;
    fn file_extension(&self) -> &'static str;
    fn detect(&self, data: &[u8]) -> bool;
    fn load(&self, data: &[u8]) -> FormatResult<Module>;
}

pub fn detect_format(data: &[u8]) -> Option<ModuleFormat> {
    if data.len() < 4 {
        return None;
    }

    let magic = &data[0..4];

    if magic == b"IMPM" {
        return Some(ModuleFormat::IT);
    }

    if magic == b"MMD0" || magic == b"MMD1" {
        return Some(ModuleFormat::Mmd);
    }

    if data.len() > 4 && magic == b"HTRA" {
        return Some(ModuleFormat::HTK);
    }

    if data.len() > 37 && &data[0..17] == b"Extended Module: " {
        return Some(ModuleFormat::XM);
    }

    if data.len() > 48 && &data[44..48] == b"SCRM" {
        return Some(ModuleFormat::S3M);
    }

    if magic == b"if" || magic == b"JN" {
        return Some(ModuleFormat::C669);
    }

    if data.len() >= 15 && &data[0..12] == b"MAS_UTrack_V" {
        return Some(ModuleFormat::Ult);
    }

    if data.len() >= 29 && &data[20..28] == b"!Scream!" {
        return Some(ModuleFormat::Stm);
    }

    if data.len() > 1084 {
        let mod_magic = &data[1080..1084];
        if is_mod_magic(mod_magic) {
            return Some(ModuleFormat::MOD);
        }
    }

    None
}

pub fn detect_instrument_format(data: &[u8]) -> Option<&'static str> {
    if data.len() >= 4 && &data[0..4] == b"HTIN" {
        Some("HTI")
    } else {
        None
    }
}

pub fn load_instrument(data: &[u8]) -> FormatResult<(Instrument, Vec<Sample>)> {
    hti::load_instrument(data)
}

pub fn save_instrument(instrument: &Instrument, samples: &[Sample]) -> FormatResult<Vec<u8>> {
    hti::save_instrument(instrument, samples)
}

fn is_mod_magic(magic: &[u8]) -> bool {
    const MOD_SIGNATURES: &[&[u8]] = &[
        b"M.K.", b"M!K!", b"FLT4", b"FLT8", b"4CHN", b"6CHN", b"8CHN", b"2CHN", b"CD81",
        b"OKTA", b"16CN", b"32CN",
    ];
    MOD_SIGNATURES.iter().any(|sig| magic == *sig)
}

pub fn load_module(data: &[u8]) -> FormatResult<Module> {
    let format = detect_format(data).ok_or_else(|| crate::errors::FormatError::InvalidHeader {
        expected: "recognizable format magic".to_string(),
        found: {
            let mut arr = [0u8; 4];
            if data.len() >= 4 {
                arr.copy_from_slice(&data[0..4]);
            }
            arr
        },
    })?;

    match format {
        ModuleFormat::IT => {
            let handler = it::ItHandler;
            handler.load(data)
        }
        ModuleFormat::XM => {
            let handler = xm::XmHandler;
            handler.load(data)
        }
        ModuleFormat::MOD => {
            let handler = modfile::ModHandler;
            handler.load(data)
        }
        ModuleFormat::S3M => {
            let handler = s3m::S3mHandler;
            handler.load(data)
        }
        ModuleFormat::HTK => {
            htk::load_module(data)
        }
        ModuleFormat::C669 => {
            let handler = c669::C669Handler;
            handler.load(data)
        }
        ModuleFormat::Mmd => {
            let handler = mmd::MmdHandler;
            handler.load(data)
        }
        ModuleFormat::Ult => {
            let handler = ult::UltHandler;
            handler.load(data)
        }
        ModuleFormat::Stm => {
            let handler = stm::StmHandler;
            handler.load(data)
        }
    }
}

pub fn save_module(module: &Module) -> Vec<u8> {
    htk::save_module(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_it_format() {
        let mut data = vec![0u8; 192];
        data[0..4].copy_from_slice(b"IMPM");
        assert_eq!(detect_format(&data), Some(ModuleFormat::IT));
    }

    #[test]
    fn detect_too_small() {
        let data = [0u8; 3];
        assert_eq!(detect_format(&data), None);
    }

    #[test]
    fn detect_unknown_format() {
        let data = vec![0xABu8; 200];
        assert_eq!(detect_format(&data), None);
    }

    #[test]
    fn detect_hti_instrument_format() {
        let mut data = vec![0u8; 12];
        data[0..4].copy_from_slice(b"HTIN");
        assert_eq!(detect_instrument_format(&data), Some("HTI"));
    }

    #[test]
    fn detect_non_hti_format() {
        let mut data = vec![0u8; 4];
        data[0..4].copy_from_slice(b"XMAD");
        assert_eq!(detect_instrument_format(&data), None);
    }

    #[test]
    fn hti_roundtrip_instrument() {
        use crate::sequencer::instrument::Instrument;
        use crate::sequencer::sample::Sample;

        let mut inst = Instrument::default();
        inst.name = "Test".to_string();
        inst.fade_out = 100;

        let samples: Vec<Sample> = vec![];

        let data = save_instrument(&inst, &samples).unwrap();
        let (loaded_inst, loaded_samples) = load_instrument(&data).unwrap();

        assert_eq!(loaded_inst.name, inst.name);
        assert_eq!(loaded_inst.fade_out, inst.fade_out);
        assert!(loaded_samples.is_empty());
    }
}
