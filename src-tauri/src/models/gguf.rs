use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use serde_json::{json, Value};

use crate::errors::{AppError, AppResult};

use super::types::GgufMetadata;

const GGUF_MAGIC: &[u8; 4] = b"GGUF";
const MAX_METADATA_BYTES: u64 = 64 * 1024 * 1024;
const MAX_METADATA_ENTRIES: u64 = 100_000;
const MAX_TENSORS: u64 = 10_000_000;
const MAX_KEY_BYTES: u64 = 4 * 1024;
const MAX_STRING_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ARRAY_ITEMS: u64 = 2_000_000;
const MAX_PREVIEW_ENTRIES: usize = 64;

pub fn inspect_gguf(path: &Path) -> AppResult<GgufMetadata> {
    let mut reader = GgufReader::open(path)?;
    let magic = reader.read_array::<4>()?;
    if &magic != GGUF_MAGIC {
        return Err(invalid("the file does not start with the GGUF magic bytes"));
    }

    let version = reader.read_u32()?;
    if !(2..=3).contains(&version) {
        return Err(invalid(format!(
            "GGUF version {version} is not supported; expected version 2 or 3"
        )));
    }
    let tensor_count = reader.read_u64()?;
    let metadata_count = reader.read_u64()?;
    if tensor_count > MAX_TENSORS {
        return Err(invalid("the tensor count exceeds the safety limit"));
    }
    if metadata_count > MAX_METADATA_ENTRIES {
        return Err(invalid("the metadata entry count exceeds the safety limit"));
    }

    let metadata_start = reader.position;
    let mut metadata = GgufMetadata {
        version,
        tensor_count,
        metadata_count,
        ..GgufMetadata::default()
    };
    let mut preview = BTreeMap::new();

    for _ in 0..metadata_count {
        let key = reader.read_string(MAX_KEY_BYTES)?;
        let value_type = reader.read_u32()?;
        let parsed = reader.read_value(value_type)?;

        match key.as_str() {
            "general.architecture" => metadata.architecture = parsed.scalar_string(),
            "general.name" => metadata.name = parsed.scalar_string(),
            "general.file_type" => {
                metadata.file_type = parsed
                    .scalar_u64()
                    .and_then(|value| u32::try_from(value).ok())
            }
            "general.parameter_count" => metadata.parameter_count = parsed.scalar_u64(),
            "tokenizer.chat_template" => {
                metadata.has_chat_template = parsed.scalar_string().is_some()
            }
            _ if key.ends_with(".context_length") => metadata.context_length = parsed.scalar_u64(),
            _ if key.ends_with(".embedding_length") => {
                metadata.embedding_length = parsed.scalar_u64()
            }
            _ if key.ends_with(".block_count") => metadata.layer_count = parsed.scalar_u64(),
            _ => {}
        }

        if preview.len() < MAX_PREVIEW_ENTRIES {
            let value = if key == "tokenizer.chat_template" {
                json!({ "present": parsed.scalar_string().is_some() })
            } else {
                parsed.preview
            };
            preview.insert(key, value);
        }
    }

    metadata.metadata_bytes = reader.position.saturating_sub(metadata_start);
    metadata.quantization = metadata.file_type.map(quantization_name);
    metadata.metadata_preview = preview;
    Ok(metadata)
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidModel(message.into())
}

fn quantization_name(file_type: u32) -> String {
    let name = match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        6 => "Q5_0",
        7 => "Q5_1",
        8 => "Q8_0",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "IQ3_XXS",
        22 => "IQ1_S",
        23 => "IQ4_NL",
        24 => "IQ3_S",
        25 => "IQ2_S",
        26 => "IQ4_XS",
        27 => "I8",
        28 => "I16",
        29 => "I32",
        30 => "I64",
        31 => "F64",
        32 => "IQ1_M",
        33 => "BF16",
        34 => "TQ1_0",
        35 => "TQ2_0",
        36 => "MXFP4",
        _ => return format!("GGML type {file_type}"),
    };
    name.to_string()
}

struct GgufReader {
    file: File,
    position: u64,
    limit: u64,
}

impl GgufReader {
    fn open(path: &Path) -> AppResult<Self> {
        let file = File::open(path)
            .map_err(|error| invalid(format!("the file could not be opened: {error}")))?;
        let length = file
            .metadata()
            .map_err(|error| invalid(format!("the file size could not be read: {error}")))?
            .len();
        Ok(Self {
            file,
            position: 0,
            limit: length.min(MAX_METADATA_BYTES),
        })
    }

    fn ensure(&self, length: u64) -> AppResult<()> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid("a metadata length overflowed"))?;
        if end > self.limit {
            return Err(invalid(
                "the GGUF header is truncated or exceeds the 64 MiB metadata limit",
            ));
        }
        Ok(())
    }

    fn read_exact(&mut self, buffer: &mut [u8]) -> AppResult<()> {
        self.ensure(buffer.len() as u64)?;
        self.file
            .read_exact(buffer)
            .map_err(|error| invalid(format!("the GGUF header is truncated: {error}")))?;
        self.position += buffer.len() as u64;
        Ok(())
    }

    fn read_array<const N: usize>(&mut self) -> AppResult<[u8; N]> {
        let mut value = [0_u8; N];
        self.read_exact(&mut value)?;
        Ok(value)
    }

    fn read_u8(&mut self) -> AppResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16(&mut self) -> AppResult<u16> {
        Ok(u16::from_le_bytes(self.read_array()?))
    }

    fn read_u32(&mut self) -> AppResult<u32> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> AppResult<u64> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    fn read_i8(&mut self) -> AppResult<i8> {
        Ok(i8::from_le_bytes(self.read_array()?))
    }

    fn read_i16(&mut self) -> AppResult<i16> {
        Ok(i16::from_le_bytes(self.read_array()?))
    }

    fn read_i32(&mut self) -> AppResult<i32> {
        Ok(i32::from_le_bytes(self.read_array()?))
    }

    fn read_i64(&mut self) -> AppResult<i64> {
        Ok(i64::from_le_bytes(self.read_array()?))
    }

    fn read_f32(&mut self) -> AppResult<f32> {
        Ok(f32::from_le_bytes(self.read_array()?))
    }

    fn read_f64(&mut self) -> AppResult<f64> {
        Ok(f64::from_le_bytes(self.read_array()?))
    }

    fn read_string(&mut self, maximum: u64) -> AppResult<String> {
        let length = self.read_u64()?;
        if length > maximum {
            return Err(invalid(format!(
                "a GGUF string exceeds the {maximum}-byte safety limit"
            )));
        }
        let length =
            usize::try_from(length).map_err(|_| invalid("a GGUF string length is invalid"))?;
        let mut bytes = vec![0_u8; length];
        self.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| invalid("GGUF metadata contains invalid UTF-8"))
    }

    fn skip(&mut self, length: u64) -> AppResult<()> {
        self.ensure(length)?;
        let offset =
            i64::try_from(length).map_err(|_| invalid("a metadata offset is too large"))?;
        self.file
            .seek(SeekFrom::Current(offset))
            .map_err(|error| invalid(format!("the GGUF metadata could not be read: {error}")))?;
        self.position += length;
        Ok(())
    }

    fn read_value(&mut self, value_type: u32) -> AppResult<ParsedValue> {
        let scalar = match value_type {
            0 => Scalar::Unsigned(self.read_u8()? as u64),
            1 => Scalar::Signed(self.read_i8()? as i64),
            2 => Scalar::Unsigned(self.read_u16()? as u64),
            3 => Scalar::Signed(self.read_i16()? as i64),
            4 => Scalar::Unsigned(self.read_u32()? as u64),
            5 => Scalar::Signed(self.read_i32()? as i64),
            6 => Scalar::Float(self.read_f32()? as f64),
            7 => Scalar::Bool(match self.read_u8()? {
                0 => false,
                1 => true,
                _ => return Err(invalid("a GGUF boolean has an invalid value")),
            }),
            8 => Scalar::String(self.read_string(MAX_STRING_BYTES)?),
            9 => return self.read_array_value(),
            10 => Scalar::Unsigned(self.read_u64()?),
            11 => Scalar::Signed(self.read_i64()?),
            12 => Scalar::Float(self.read_f64()?),
            _ => {
                return Err(invalid(format!(
                    "GGUF metadata uses unknown value type {value_type}"
                )))
            }
        };
        Ok(ParsedValue::from_scalar(scalar))
    }

    fn read_array_value(&mut self) -> AppResult<ParsedValue> {
        let element_type = self.read_u32()?;
        let count = self.read_u64()?;
        if count > MAX_ARRAY_ITEMS {
            return Err(invalid(
                "a GGUF metadata array exceeds the item safety limit",
            ));
        }
        if element_type == 9 || element_type > 12 {
            return Err(invalid(format!(
                "GGUF metadata array uses unsupported type {element_type}"
            )));
        }

        if let Some(width) = fixed_width(element_type) {
            let bytes = count
                .checked_mul(width)
                .ok_or_else(|| invalid("a GGUF metadata array length overflowed"))?;
            self.skip(bytes)?;
        } else {
            for _ in 0..count {
                let length = self.read_u64()?;
                if length > MAX_STRING_BYTES {
                    return Err(invalid("a string in a GGUF metadata array is too large"));
                }
                self.skip(length)?;
            }
        }
        Ok(ParsedValue {
            scalar: None,
            preview: json!({ "arrayType": element_type, "count": count }),
        })
    }
}

fn fixed_width(value_type: u32) -> Option<u64> {
    match value_type {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        8 => None,
        10..=12 => Some(8),
        _ => None,
    }
}

enum Scalar {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Bool(bool),
    String(String),
}

struct ParsedValue {
    scalar: Option<Scalar>,
    preview: Value,
}

impl ParsedValue {
    fn from_scalar(scalar: Scalar) -> Self {
        let preview = match &scalar {
            Scalar::Unsigned(value) => json!(value),
            Scalar::Signed(value) => json!(value),
            Scalar::Float(value) => json!(value),
            Scalar::Bool(value) => json!(value),
            Scalar::String(value) => {
                let preview: String = value.chars().take(256).collect();
                if preview.len() < value.len() {
                    json!({ "preview": preview, "truncated": true, "bytes": value.len() })
                } else {
                    json!(value)
                }
            }
        };
        Self {
            scalar: Some(scalar),
            preview,
        }
    }

    fn scalar_string(&self) -> Option<String> {
        match &self.scalar {
            Some(Scalar::String(value)) => Some(value.clone()),
            _ => None,
        }
    }

    fn scalar_u64(&self) -> Option<u64> {
        match self.scalar {
            Some(Scalar::Unsigned(value)) => Some(value),
            Some(Scalar::Signed(value)) if value >= 0 => Some(value as u64),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) fn write_test_gguf(path: &Path) {
    use std::io::Write;

    fn write_string(file: &mut File, value: &str) {
        file.write_all(&(value.len() as u64).to_le_bytes()).unwrap();
        file.write_all(value.as_bytes()).unwrap();
    }
    fn write_string_value(file: &mut File, key: &str, value: &str) {
        write_string(file, key);
        file.write_all(&8_u32.to_le_bytes()).unwrap();
        write_string(file, value);
    }
    fn write_u32_value(file: &mut File, key: &str, value: u32) {
        write_string(file, key);
        file.write_all(&4_u32.to_le_bytes()).unwrap();
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    fn write_u64_value(file: &mut File, key: &str, value: u64) {
        write_string(file, key);
        file.write_all(&10_u32.to_le_bytes()).unwrap();
        file.write_all(&value.to_le_bytes()).unwrap();
    }

    let mut file = File::create(path).unwrap();
    file.write_all(GGUF_MAGIC).unwrap();
    file.write_all(&3_u32.to_le_bytes()).unwrap();
    file.write_all(&0_u64.to_le_bytes()).unwrap();
    file.write_all(&8_u64.to_le_bytes()).unwrap();
    write_string_value(&mut file, "general.architecture", "llama");
    write_string_value(&mut file, "general.name", "Test Model");
    write_u32_value(&mut file, "general.file_type", 15);
    write_u64_value(&mut file, "general.parameter_count", 7_000_000_000);
    write_u32_value(&mut file, "llama.context_length", 32_768);
    write_u32_value(&mut file, "llama.embedding_length", 4_096);
    write_u32_value(&mut file, "llama.block_count", 32);
    write_string_value(&mut file, "tokenizer.chat_template", "{{ messages }}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("neuraloc-{name}-{}.gguf", Uuid::new_v4()))
    }

    #[test]
    fn reads_bounded_useful_metadata() {
        let path = temp_file("valid");
        write_test_gguf(&path);
        let metadata = inspect_gguf(&path).unwrap();
        assert_eq!(metadata.architecture.as_deref(), Some("llama"));
        assert_eq!(metadata.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(metadata.context_length, Some(32_768));
        assert_eq!(metadata.layer_count, Some(32));
        assert!(metadata.has_chat_template);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_wrong_magic_and_truncated_headers() {
        for bytes in [b"NOPE".as_slice(), b"GGUF\x03".as_slice()] {
            let path = temp_file("invalid");
            std::fs::write(&path, bytes).unwrap();
            assert!(inspect_gguf(&path).is_err());
            std::fs::remove_file(path).unwrap();
        }
    }
}
