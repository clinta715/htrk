pub mod common;
pub mod it;
pub mod s3m;
pub mod wav;
pub mod xm;
pub mod modfile;

use crate::errors::FormatResult;
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

    if data.len() > 37 && &data[0..17] == b"Extended Module: " {
        return Some(ModuleFormat::XM);
    }

    if data.len() > 48 && &data[44..48] == b"SCRM" {
        return Some(ModuleFormat::S3M);
    }

    if data.len() > 1084 {
        let mod_magic = &data[1080..1084];
        if is_mod_magic(mod_magic) {
            return Some(ModuleFormat::MOD);
        }
    }

    None
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
        expected: "recognizable format magic",
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
    }
}

pub fn save_module(module: &Module, format: ModuleFormat) -> Vec<u8> {
    match format {
        ModuleFormat::IT => it::save_module(module),
        ModuleFormat::XM => xm::save_module(module),
        ModuleFormat::S3M => s3m::save_module(module),
        ModuleFormat::MOD => modfile::save_module(module),
    }
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
}
