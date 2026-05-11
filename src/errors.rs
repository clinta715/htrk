use std::fmt;
use std::io;
use std::str;

#[derive(Debug)]
#[allow(dead_code)]
pub enum FormatError {
    Io(io::Error),
    InvalidHeader { expected: String, found: [u8; 4] },
    TruncatedFile { expected_size: usize, actual_size: usize },
    #[allow(dead_code)]
    UnsupportedVersion { version: u16 },
    #[allow(dead_code)]
    InvalidPatternIndex { index: usize, max: usize },
    #[allow(dead_code)]
    InvalidSampleIndex { index: usize, max: usize },
    DecompressionFailed(String),
    Utf8Error(str::Utf8Error),
    Bincode(String),
    #[allow(dead_code)]
    SerializeError(String),
    #[allow(dead_code)]
    ParseError(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "I/O error: {}", e),
            FormatError::InvalidHeader { expected, found } => {
                write!(f, "Invalid header: expected {}, found {:?}", expected, String::from_utf8_lossy(found))
            }
            FormatError::TruncatedFile { expected_size, actual_size } => {
                write!(f, "Truncated file: expected {} bytes, got {}", expected_size, actual_size)
            }
            FormatError::UnsupportedVersion { version } => {
                write!(f, "Unsupported version: {}", version)
            }
            FormatError::InvalidPatternIndex { index, max } => {
                write!(f, "Invalid pattern index {}: max {}", index, max)
            }
            FormatError::InvalidSampleIndex { index, max } => {
                write!(f, "Invalid sample index {}: max {}", index, max)
            }
            FormatError::DecompressionFailed(msg) => {
                write!(f, "Decompression failed: {}", msg)
            }
            FormatError::Utf8Error(e) => write!(f, "UTF-8 error: {}", e),
            FormatError::Bincode(msg) => write!(f, "Bincode error: {}", msg),
            FormatError::SerializeError(msg) => write!(f, "Serialize error: {}", msg),
            FormatError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for FormatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FormatError::Io(e) => Some(e),
            FormatError::Utf8Error(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for FormatError {
    fn from(e: io::Error) -> Self {
        FormatError::Io(e)
    }
}

impl From<str::Utf8Error> for FormatError {
    fn from(e: str::Utf8Error) -> Self {
        FormatError::Utf8Error(e)
    }
}

impl From<bincode::Error> for FormatError {
    fn from(e: bincode::Error) -> Self {
        FormatError::Bincode(e.to_string())
    }
}

impl From<hound::Error> for FormatError {
    fn from(e: hound::Error) -> Self {
        FormatError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}

pub type FormatResult<T> = Result<T, FormatError>;

#[derive(Debug)]
#[allow(dead_code)]
pub enum AudioError {
    NoDeviceAvailable,
    DeviceOpenFailed(String),
    UnsupportedSampleRate { requested: u32, available: Vec<u32> },
    StreamCreationFailed(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::NoDeviceAvailable => write!(f, "No audio device available"),
            AudioError::DeviceOpenFailed(msg) => write!(f, "Device open failed: {}", msg),
            AudioError::UnsupportedSampleRate { requested, available } => {
                write!(f, "Unsupported sample rate {}: available {:?}", requested, available)
            }
            AudioError::StreamCreationFailed(msg) => {
                write!(f, "Stream creation failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for AudioError {}

#[allow(dead_code)]
pub type AudioResult<T> = Result<T, AudioError>;

#[derive(Debug)]
#[allow(dead_code)]
pub enum EditError {
    NoSelection,
    CannotPasteDifferentChannels,
    PatternFull,
    InvalidNoteValue,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoSelection => write!(f, "No selection"),
            EditError::CannotPasteDifferentChannels => write!(f, "Cannot paste: different channel count"),
            EditError::PatternFull => write!(f, "Pattern is full"),
            EditError::InvalidNoteValue => write!(f, "Invalid note value"),
        }
    }
}

impl std::error::Error for EditError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_error_display() {
        let e = FormatError::InvalidHeader {
            expected: "IMPM".to_string(),
            found: [0x00, 0x00, 0x00, 0x00],
        };
        let msg = format!("{}", e);
        assert!(msg.contains("IMPM"));
    }

    #[test]
    fn audio_error_display() {
        let e = AudioError::NoDeviceAvailable;
        let msg = format!("{}", e);
        assert!(msg.contains("No audio device"));
    }

    #[test]
    fn edit_error_display() {
        let e = EditError::NoSelection;
        let msg = format!("{}", e);
        assert!(msg.contains("No selection"));
    }

    #[test]
    fn format_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let format_err: FormatError = io_err.into();
        match format_err {
            FormatError::Io(_) => {}
            _ => panic!("Expected Io variant"),
        }
    }
}
