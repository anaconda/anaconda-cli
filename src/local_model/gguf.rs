//! Minimal GGUF metadata reader.
//!
//! Only reads the key/value header to find the model's training context
//! length (`<arch>.context_length`), which is what llama-server uses by default.

// Only used by `ana lm run`, which requires tool installation.
#![cfg_attr(not(tool_install), allow(dead_code))]

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const GGUF_MAGIC: &[u8; 4] = b"GGUF";

// GGUF metadata value types.
const T_U8: u32 = 0;
const T_I8: u32 = 1;
const T_U16: u32 = 2;
const T_I16: u32 = 3;
const T_U32: u32 = 4;
const T_I32: u32 = 5;
const T_F32: u32 = 6;
const T_BOOL: u32 = 7;
const T_STRING: u32 = 8;
const T_ARRAY: u32 = 9;
const T_U64: u32 = 10;
const T_I64: u32 = 11;
const T_F64: u32 = 12;

/// Read the training context length from a GGUF file's metadata.
///
/// Returns `None` if the file can't be read, isn't GGUF v2+, or doesn't
/// declare a context length.
pub fn context_length(path: &Path) -> Option<u64> {
    let file = File::open(path).ok()?;
    read_context_length(&mut BufReader::new(file))
}

fn read_context_length(r: &mut impl Read) -> Option<u64> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).ok()?;
    if &magic != GGUF_MAGIC {
        return None;
    }
    // v1 used 32-bit lengths; only v2+ is supported.
    if read_u32(r)? < 2 {
        return None;
    }
    let _tensor_count = read_u64(r)?;
    let kv_count = read_u64(r)?;

    for _ in 0..kv_count {
        let key = read_string(r)?;
        let ty = read_u32(r)?;
        if key.ends_with(".context_length") {
            return read_uint(r, ty);
        }
        skip_value(r, ty)?;
    }
    None
}

fn read_u32(r: &mut impl Read) -> Option<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> Option<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

fn read_string(r: &mut impl Read) -> Option<String> {
    let len = read_u64(r)?;
    // Guard against corrupt lengths; metadata keys are short.
    if len > 1 << 20 {
        return None;
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// Read an unsigned integer value of the given type.
fn read_uint(r: &mut impl Read, ty: u32) -> Option<u64> {
    match ty {
        T_U32 | T_I32 => read_u32(r).map(u64::from),
        T_U64 | T_I64 => read_u64(r),
        _ => None,
    }
}

fn skip(r: &mut impl Read, n: u64) -> Option<()> {
    let copied = std::io::copy(&mut r.take(n), &mut std::io::sink()).ok()?;
    (copied == n).then_some(())
}

fn fixed_size(ty: u32) -> Option<u64> {
    match ty {
        T_U8 | T_I8 | T_BOOL => Some(1),
        T_U16 | T_I16 => Some(2),
        T_U32 | T_I32 | T_F32 => Some(4),
        T_U64 | T_I64 | T_F64 => Some(8),
        _ => None,
    }
}

fn skip_value(r: &mut impl Read, ty: u32) -> Option<()> {
    if let Some(size) = fixed_size(ty) {
        return skip(r, size);
    }
    match ty {
        T_STRING => {
            let len = read_u64(r)?;
            skip(r, len)
        }
        T_ARRAY => {
            let elem_ty = read_u32(r)?;
            let count = read_u64(r)?;
            if let Some(size) = fixed_size(elem_ty) {
                return skip(r, size.checked_mul(count)?);
            }
            for _ in 0..count {
                skip_value(r, elem_ty)?;
            }
            Some(())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gguf_string(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    /// Build a GGUF header with a string, a string array, and a context length.
    fn sample_header(ctx_type: u32, ctx: u64) -> Vec<u8> {
        let mut v = b"GGUF".to_vec();
        v.extend(3u32.to_le_bytes()); // version
        v.extend(0u64.to_le_bytes()); // tensor count
        v.extend(3u64.to_le_bytes()); // kv count

        v.extend(gguf_string("general.architecture"));
        v.extend(T_STRING.to_le_bytes());
        v.extend(gguf_string("qwen2"));

        v.extend(gguf_string("tokenizer.ggml.tokens"));
        v.extend(T_ARRAY.to_le_bytes());
        v.extend(T_STRING.to_le_bytes());
        v.extend(2u64.to_le_bytes());
        v.extend(gguf_string("a"));
        v.extend(gguf_string("bc"));

        v.extend(gguf_string("qwen2.context_length"));
        v.extend(ctx_type.to_le_bytes());
        if ctx_type == T_U64 {
            v.extend(ctx.to_le_bytes());
        } else {
            v.extend((ctx as u32).to_le_bytes());
        }
        v
    }

    #[test]
    fn test_reads_u32_context_length() {
        let data = sample_header(T_U32, 32768);
        assert_eq!(read_context_length(&mut data.as_slice()), Some(32768));
    }

    #[test]
    fn test_reads_u64_context_length() {
        let data = sample_header(T_U64, 131072);
        assert_eq!(read_context_length(&mut data.as_slice()), Some(131072));
    }

    #[test]
    fn test_rejects_non_gguf() {
        assert_eq!(read_context_length(&mut b"NOPE....".as_slice()), None);
    }

    #[test]
    fn test_truncated_returns_none() {
        let data = sample_header(T_U32, 32768);
        let truncated = &data[..data.len() - 2];
        assert_eq!(read_context_length(&mut &truncated[..]), None);
    }
}
