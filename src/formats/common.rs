use crate::errors::FormatError;
use crate::errors::FormatResult;

pub fn read_u8(data: &[u8], offset: &mut usize) -> FormatResult<u8> {
    if *offset >= data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + 1,
            actual_size: data.len(),
        });
    }
    let val = data[*offset];
    *offset += 1;
    Ok(val)
}

pub fn read_u16_le(data: &[u8], offset: &mut usize) -> FormatResult<u16> {
    if *offset + 2 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + 2,
            actual_size: data.len(),
        });
    }
    let val = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    Ok(val)
}

pub fn read_u32_le(data: &[u8], offset: &mut usize) -> FormatResult<u32> {
    if *offset + 4 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + 4,
            actual_size: data.len(),
        });
    }
    let val = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(val)
}

pub fn read_u16_be(data: &[u8], offset: &mut usize) -> FormatResult<u16> {
    if *offset + 2 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + 2,
            actual_size: data.len(),
        });
    }
    let val = u16::from_be_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    Ok(val)
}

pub fn read_u32_be(data: &[u8], offset: &mut usize) -> FormatResult<u32> {
    if *offset + 4 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + 4,
            actual_size: data.len(),
        });
    }
    let val = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(val)
}

pub fn read_string(data: &[u8], offset: &mut usize, len: usize) -> FormatResult<String> {
    if *offset + len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + len,
            actual_size: data.len(),
        });
    }
    let s = &data[*offset..*offset + len];
    *offset += len;
    Ok(std::str::from_utf8(s)
        .unwrap_or("")
        .trim_end_matches('\0')
        .trim_end()
        .to_string())
}

pub fn read_bytes<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> FormatResult<&'a [u8]> {
    if *offset + len > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: *offset + len,
            actual_size: data.len(),
        });
    }
    let slice = &data[*offset..*offset + len];
    *offset += len;
    Ok(slice)
}

pub fn check_magic(data: &[u8], offset: usize, expected: &'static [u8; 4]) -> FormatResult<()> {
    if offset + 4 > data.len() {
        return Err(FormatError::TruncatedFile {
            expected_size: offset + 4,
            actual_size: data.len(),
        });
    }
    let found = &data[offset..offset + 4];
    if found != expected {
        let mut arr = [0u8; 4];
        arr.copy_from_slice(found);
        return Err(FormatError::InvalidHeader {
            expected: std::str::from_utf8(expected).unwrap_or("????").to_string(),
            found: arr,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_u8_basic() {
        let data = [0x42];
        let mut offset = 0;
        assert_eq!(read_u8(&data, &mut offset).unwrap(), 0x42);
        assert_eq!(offset, 1);
    }

    #[test]
    fn read_u16_le_basic() {
        let data = [0x34, 0x12];
        let mut offset = 0;
        assert_eq!(read_u16_le(&data, &mut offset).unwrap(), 0x1234);
        assert_eq!(offset, 2);
    }

    #[test]
    fn read_u32_le_basic() {
        let data = [0x78, 0x56, 0x34, 0x12];
        let mut offset = 0;
        assert_eq!(read_u32_le(&data, &mut offset).unwrap(), 0x12345678);
        assert_eq!(offset, 4);
    }

    #[test]
    fn read_u16_be_basic() {
        let data = [0x12, 0x34];
        let mut offset = 0;
        assert_eq!(read_u16_be(&data, &mut offset).unwrap(), 0x1234);
        assert_eq!(offset, 2);
    }

    #[test]
    fn read_u32_be_basic() {
        let data = [0x12, 0x34, 0x56, 0x78];
        let mut offset = 0;
        assert_eq!(read_u32_be(&data, &mut offset).unwrap(), 0x12345678);
        assert_eq!(offset, 4);
    }

    #[test]
    fn read_string_trims_nulls() {
        let data = b"hello\x00\x00\x00\x00\x00";
        let mut offset = 0;
        let s = read_string(data, &mut offset, 10).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn read_past_end_errors() {
        let data = [0x01];
        let mut offset = 1;
        assert!(read_u8(&data, &mut offset).is_err());
    }

    #[test]
    fn check_magic_valid() {
        let data = b"IMPM";
        assert!(check_magic(data, 0, b"IMPM").is_ok());
    }

    #[test]
    fn check_magic_invalid() {
        let data = b"XXXX";
        assert!(check_magic(data, 0, b"IMPM").is_err());
    }
}
