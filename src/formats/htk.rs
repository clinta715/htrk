use crate::errors::{FormatError, FormatResult};
use crate::sequencer::module::ModuleFormat;
use crate::sequencer::Module;

const HTK_VERSION: u32 = 5;
const HTK_MAGIC: &[u8; 4] = b"HTRA";

pub fn save_module(module: &Module) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(HTK_MAGIC);
    data.extend_from_slice(&HTK_VERSION.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    let encoded = bincode::serialize(module).unwrap_or_default();
    data.extend_from_slice(&encoded);
    data
}

pub fn load_module(data: &[u8]) -> FormatResult<Module> {
    if data.len() < 12 || &data[0..4] != HTK_MAGIC {
        return Err(FormatError::InvalidHeader {
            expected: "HTRA magic".to_string(),
            found: {
                let mut arr = [0u8; 4];
                if data.len() >= 4 {
                    arr.copy_from_slice(&data[0..4]);
                }
                arr
            },
        });
    }
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let _flags = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    
    if version > HTK_VERSION {
        return Err(FormatError::InvalidHeader {
            expected: format!("HTK version <= {}", HTK_VERSION),
            found: version.to_le_bytes(),
        });
    }
    
    let payload = &data[12..];
    let mut module: Module = bincode::deserialize(payload)?;
    module.format = ModuleFormat::HTK;
    Ok(module)
}