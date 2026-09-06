//! Minimal GGUF **header** parser (ADR-0008). Reads just enough of the leading
//! bytes to show a model's architecture, quantization, and context length in the
//! picker — never the tensor data. Works on a partial buffer: if the slice ends
//! mid-structure it returns [`GgufParse::Incomplete`] and the caller fetches a
//! larger range.
//!
//! Format reference: <https://github.com/ggerganov/ggml/blob/master/docs/gguf.md>

use crate::ipc::AppError;

const MAGIC: &[u8; 4] = b"GGUF";

/// The subset of GGUF header metadata the picker needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufHeader {
    /// GGUF container version (2 or 3).
    pub version: u32,
    /// `general.architecture` (e.g. `llama`, `qwen2`).
    pub architecture: String,
    /// A human quant label derived from `general.file_type` (e.g. `Q4_K_M`).
    pub quant: Option<String>,
    /// `<arch>.context_length`.
    pub context_length: Option<u32>,
    /// `<arch>.block_count` (transformer layers).
    pub block_count: Option<u32>,
}

/// Result of parsing a (possibly truncated) buffer.
#[derive(Debug)]
pub enum GgufParse {
    /// Everything needed was found.
    Header(GgufHeader),
    /// The buffer ran out before the needed keys — fetch more bytes.
    Incomplete,
}

/// Parse the header from `buf` (the first bytes of a GGUF file).
///
/// # Errors
/// [`AppError::Validation`] if the magic is wrong or the version is unsupported.
pub fn parse_header(buf: &[u8]) -> Result<GgufParse, AppError> {
    let mut r = Reader::new(buf);

    let Some(magic) = r.take(4) else {
        return Ok(GgufParse::Incomplete);
    };
    if magic != MAGIC {
        return Err(AppError::Validation(
            "not a GGUF file (bad magic)".to_owned(),
        ));
    }
    let Some(version) = r.u32() else {
        return Ok(GgufParse::Incomplete);
    };
    if version != 2 && version != 3 {
        return Err(AppError::Validation(format!(
            "unsupported GGUF version {version}"
        )));
    }
    let (Some(_tensor_count), Some(kv_count)) = (r.u64(), r.u64()) else {
        return Ok(GgufParse::Incomplete);
    };

    let mut architecture: Option<String> = None;
    let mut file_type: Option<u32> = None;
    let mut context_length: Option<u32> = None;
    let mut block_count: Option<u32> = None;

    for _ in 0..kv_count {
        let Some(key) = r.gguf_string() else {
            return Ok(GgufParse::Incomplete);
        };
        let Some(value) = r.value() else {
            return Ok(GgufParse::Incomplete);
        };

        match key.as_str() {
            "general.architecture" => {
                if let Value::Str(s) = value {
                    architecture = Some(s);
                }
            }
            "general.file_type" => {
                if let Some(n) = value.as_u32() {
                    file_type = Some(n);
                }
            }
            k if k.ends_with(".context_length") => {
                if let Some(n) = value.as_u32() {
                    context_length = Some(n);
                }
            }
            k if k.ends_with(".block_count") => {
                if let Some(n) = value.as_u32() {
                    block_count = Some(n);
                }
            }
            _ => {}
        }

        // Stop early once we have the architecture-scoped keys too.
        if architecture.is_some()
            && file_type.is_some()
            && context_length.is_some()
            && block_count.is_some()
        {
            break;
        }
    }

    let Some(architecture) = architecture else {
        // Ran the whole KV list without finding the architecture — either the
        // buffer was too short (we'd have hit Incomplete above) or it's a very
        // unusual file. Treat "no architecture after a full pass" as incomplete
        // only if we never reached the end cleanly; here we did, so it's a real
        // gap.
        return Err(AppError::Validation(
            "GGUF header has no general.architecture".to_owned(),
        ));
    };

    Ok(GgufParse::Header(GgufHeader {
        version,
        architecture,
        quant: file_type.map(quant_label),
        context_length,
        block_count,
    }))
}

/// `general.file_type` → a human quant label.
fn quant_label(file_type: u32) -> String {
    let s = match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "Q8_K",
        _ => return format!("ftype{file_type}"),
    };
    s.to_owned()
}

// ---------------------------------------------------------------- byte reader

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

#[allow(dead_code)] // variants exist to consume the right byte count when skipping
enum Value {
    U(u64),
    I(i64),
    F(f64),
    Bool(bool),
    Str(String),
    /// An array we parsed past but don't use.
    Skipped,
}

impl Value {
    fn as_u32(&self) -> Option<u32> {
        match self {
            Self::U(n) => u32::try_from(*n).ok(),
            Self::I(n) => u32::try_from(*n).ok(),
            _ => None,
        }
    }
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }

    fn i64(&mut self) -> Option<i64> {
        self.take(8)
            .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
    }

    fn gguf_string(&mut self) -> Option<String> {
        let len = usize::try_from(self.u64()?).ok()?;
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Read one GGUF metadata value (any type). Returns `None` if the buffer is
    /// too short.
    fn value(&mut self) -> Option<Value> {
        let ty = self.u32()?;
        self.value_of_type(ty)
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]
    fn value_of_type(&mut self, ty: u32) -> Option<Value> {
        Some(match ty {
            0 => Value::U(u64::from(*self.take(1)?.first()?)), // u8
            1 => Value::I(i64::from(self.take(1)?[0] as i8)),  // i8
            2 => Value::U(u64::from(u16::from_le_bytes(
                self.take(2)?.try_into().unwrap(),
            ))),
            3 => Value::I(i64::from(i16::from_le_bytes(
                self.take(2)?.try_into().unwrap(),
            ))),
            4 => Value::U(u64::from(self.u32()?)),
            5 => Value::I(i64::from(self.i32()?)),
            6 => Value::F(f64::from(f32::from_le_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            7 => Value::Bool(self.take(1)?[0] != 0),
            8 => Value::Str(self.gguf_string()?),
            9 => {
                // array: elem type + count + elements
                let elem_ty = self.u32()?;
                let count = self.u64()?;
                for _ in 0..count {
                    self.value_of_type(elem_ty)?;
                }
                Value::Skipped
            }
            10 => Value::U(self.u64()?),
            11 => Value::I(self.i64()?),
            12 => Value::F(f64::from_le_bytes(self.take(8)?.try_into().unwrap())),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid GGUF header buffer for tests.
    fn build(arch: &str, file_type: u32, ctx: u64, blocks: u64) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&3u32.to_le_bytes()); // version
        b.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
        b.extend_from_slice(&4u64.to_le_bytes()); // kv_count

        let kv_str = |key: &str, val: &str, out: &mut Vec<u8>| {
            out.extend_from_slice(&(key.len() as u64).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&8u32.to_le_bytes()); // type: string
            out.extend_from_slice(&(val.len() as u64).to_le_bytes());
            out.extend_from_slice(val.as_bytes());
        };
        let kv_u32 = |key: &str, val: u32, out: &mut Vec<u8>| {
            out.extend_from_slice(&(key.len() as u64).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&4u32.to_le_bytes()); // type: u32
            out.extend_from_slice(&val.to_le_bytes());
        };
        let kv_u64 = |key: &str, val: u64, out: &mut Vec<u8>| {
            out.extend_from_slice(&(key.len() as u64).to_le_bytes());
            out.extend_from_slice(key.as_bytes());
            out.extend_from_slice(&10u32.to_le_bytes()); // type: u64
            out.extend_from_slice(&val.to_le_bytes());
        };

        kv_str("general.architecture", arch, &mut b);
        kv_u32("general.file_type", file_type, &mut b);
        kv_u64(&format!("{arch}.context_length"), ctx, &mut b);
        kv_u64(&format!("{arch}.block_count"), blocks, &mut b);
        b
    }

    #[test]
    fn parses_a_complete_header() {
        let buf = build("llama", 15, 8192, 32);
        let GgufParse::Header(h) = parse_header(&buf).unwrap() else {
            panic!("expected a header");
        };
        assert_eq!(h.version, 3);
        assert_eq!(h.architecture, "llama");
        assert_eq!(h.quant.as_deref(), Some("Q4_K_M"));
        assert_eq!(h.context_length, Some(8192));
        assert_eq!(h.block_count, Some(32));
    }

    #[test]
    fn a_truncated_buffer_is_incomplete() {
        let buf = build("qwen2", 18, 32768, 28);
        assert!(matches!(
            parse_header(&buf[..20]).unwrap(),
            GgufParse::Incomplete
        ));
        assert!(matches!(parse_header(&[]).unwrap(), GgufParse::Incomplete));
    }

    #[test]
    fn bad_magic_is_an_error() {
        assert!(parse_header(b"NOPEnot a gguf file at all........").is_err());
    }

    #[test]
    fn unknown_file_type_gets_a_fallback_label() {
        let buf = build("llama", 999, 4096, 26);
        let GgufParse::Header(h) = parse_header(&buf).unwrap() else {
            panic!()
        };
        assert_eq!(h.quant.as_deref(), Some("ftype999"));
    }
}
