//! Safe interfaces to bounded text kernels. SIMD is optional; every kernel has a portable fallback.

#[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
mod wasm;
#[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
pub use wasm::{append_utf16, from_utf16_lossy};

#[cfg(all(feature = "simd", any(target_arch = "aarch64", target_arch = "x86_64")))]
mod native;

/// Length of the leading ASCII run, including controls. Never reads past input.
#[inline]
pub fn ascii_prefix(input: &[u8]) -> usize {
  let mut i = 0;
  #[cfg(all(feature = "simd", target_arch = "aarch64"))]
  {
    use std::arch::aarch64::*;
    // SAFETY: both unaligned loads fit in the current 32-byte block.
    unsafe {
      while input.len() - i >= 32 {
        let bytes = vorrq_u8(
          vld1q_u8(input.as_ptr().add(i)),
          vld1q_u8(input.as_ptr().add(i + 16)),
        );
        if vmaxvq_u8(bytes) >= 128 {
          break;
        }
        i += 32;
      }
    }
  }
  #[cfg(all(feature = "simd", target_arch = "x86_64"))]
  {
    use std::arch::x86_64::*;
    // SAFETY: SSE2 is baseline; both loads stay within the slice.
    unsafe {
      while input.len() - i >= 32 {
        let bytes = _mm_or_si128(
          _mm_loadu_si128(input.as_ptr().add(i).cast()),
          _mm_loadu_si128(input.as_ptr().add(i + 16).cast()),
        );
        if _mm_movemask_epi8(bytes) != 0 {
          break;
        }
        i += 32;
      }
    }
  }
  #[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
  {
    use std::arch::wasm32::*;
    // SAFETY: both loads stay within the current block.
    unsafe {
      while input.len() - i >= 32 {
        let bytes = v128_or(
          v128_load(input.as_ptr().add(i).cast()),
          v128_load(input.as_ptr().add(i + 16).cast()),
        );
        if i8x16_bitmask(bytes) != 0 {
          break;
        }
        i += 32;
      }
    }
  }
  #[cfg(not(all(
    feature = "simd",
    any(
      target_arch = "aarch64",
      target_arch = "x86_64",
      all(target_arch = "wasm32", target_feature = "simd128")
    )
  )))]
  while input.len() - i >= 32 && input[i..i + 32].is_ascii() {
    i += 32;
  }
  i + input[i..]
    .iter()
    .position(|b| !b.is_ascii())
    .unwrap_or(input.len() - i)
}

/// Decode phrase scratch data. Sample both ends to avoid the measured mixed
/// text regression of unconditional count+transcode. All output is owned.
pub fn decode(input: &str) -> Vec<char> {
  #[cfg(all(
    feature = "simd",
    not(target_family = "wasm"),
    any(target_arch = "aarch64", target_arch = "x86_64")
  ))]
  if input.len() >= 512
    && input.chars().take(32).filter(|c| c.len_utf8() == 3).count() >= 28
    && input
      .chars()
      .rev()
      .take(32)
      .filter(|c| c.len_utf8() == 3)
      .count()
      >= 28
  {
    return native::decode(input);
  }
  input.chars().collect()
}

/// Compare four packed trie labels. Zero padding cannot match a nonzero label.
#[inline]
pub fn find4(labels: &[u32; 4], code: u32) -> Option<usize> {
  #[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
  {
    use std::arch::wasm32::*;
    // SAFETY: labels contains a full unaligned 16-byte block.
    unsafe {
      let eq = i32x4_eq(v128_load(labels.as_ptr().cast()), i32x4_splat(code as i32));
      let mask = i32x4_bitmask(eq);
      return (mask != 0).then(|| mask.trailing_zeros() as usize);
    }
  }
  #[cfg(all(feature = "simd", target_arch = "aarch64"))]
  {
    use std::arch::aarch64::*;
    // SAFETY: the array contains a full 16-byte block, NEON is baseline.
    unsafe {
      let eq = vceqq_u32(vld1q_u32(labels.as_ptr()), vdupq_n_u32(code));
      let bits = vreinterpret_u64_u16(vmovn_u32(eq));
      let mask = vget_lane_u64(bits, 0);
      return (mask != 0).then(|| mask.trailing_zeros() as usize / 16);
    }
  }
  #[cfg(all(feature = "simd", target_arch = "x86_64"))]
  {
    use std::arch::x86_64::*;
    // SAFETY: unaligned load stays in the array, SSE2 is baseline.
    unsafe {
      let eq = _mm_cmpeq_epi32(
        _mm_loadu_si128(labels.as_ptr().cast()),
        _mm_set1_epi32(code as i32),
      );
      let mask = _mm_movemask_epi8(eq) as u32;
      return (mask != 0).then(|| mask.trailing_zeros() as usize / 4);
    }
  }
  #[allow(unreachable_code)]
  labels.iter().position(|&label| label == code)
}

/// Locate an ASCII byte that must be escaped inside a JSON string.
#[inline]
pub fn json_escape(input: &[u8]) -> Option<usize> {
  #[allow(unused_mut)] // Only SIMD backends advance the block cursor.
  let mut start = 0;
  #[cfg(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128"))]
  if input.len() >= 64 {
    use std::arch::wasm32::*;
    // SAFETY: every load is bounded by a complete block.
    unsafe {
      while input.len() - start >= 16 {
        let bytes = v128_load(input.as_ptr().add(start).cast());
        let escapes = v128_or(
          u8x16_lt(bytes, u8x16_splat(32)),
          v128_or(
            u8x16_eq(bytes, u8x16_splat(b'"')),
            u8x16_eq(bytes, u8x16_splat(b'\\')),
          ),
        );
        let mask = i8x16_bitmask(escapes);
        if mask != 0 {
          return Some(start + mask.trailing_zeros() as usize);
        }
        start += 16;
      }
    }
  }
  #[cfg(all(feature = "simd", target_arch = "aarch64"))]
  if input.len() >= 64 {
    use std::arch::aarch64::*;
    // SAFETY: NEON is part of the supported aarch64 baseline. Each unaligned
    // load is guarded by a full 16-byte block; no bytes past the slice are read.
    unsafe {
      while input.len() - start >= 16 {
        let bytes = vld1q_u8(input.as_ptr().add(start));
        let escapes = vorrq_u8(
          vcltq_u8(bytes, vdupq_n_u8(32)),
          vorrq_u8(
            vceqq_u8(bytes, vdupq_n_u8(b'"')),
            vceqq_u8(bytes, vdupq_n_u8(b'\\')),
          ),
        );
        if vmaxvq_u8(escapes) != 0 {
          break;
        }
        start += 16;
      }
    }
  }
  #[cfg(all(feature = "simd", target_arch = "x86_64"))]
  if input.len() >= 64 {
    use std::arch::x86_64::*;
    // SAFETY: SSE2 is baseline on x86_64. Loads stay within the slice and do
    // not require alignment. Masking the high bits makes the control check
    // unsigned, so non-ASCII UTF-8 bytes are never mistaken for controls.
    unsafe {
      while input.len() - start >= 16 {
        let bytes = _mm_loadu_si128(input.as_ptr().add(start).cast());
        let escapes = _mm_or_si128(
          _mm_cmpeq_epi8(
            _mm_and_si128(bytes, _mm_set1_epi8(-32)),
            _mm_setzero_si128(),
          ),
          _mm_or_si128(
            _mm_cmpeq_epi8(bytes, _mm_set1_epi8(b'"' as i8)),
            _mm_cmpeq_epi8(bytes, _mm_set1_epi8(b'\\' as i8)),
          ),
        );
        let mask = _mm_movemask_epi8(escapes) as u32;
        if mask != 0 {
          return Some(start + mask.trailing_zeros() as usize);
        }
        start += 16;
      }
    }
  }
  input[start..]
    .iter()
    .position(|&byte| byte < 32 || byte == b'"' || byte == b'\\')
    .map(|i| start + i)
}

/// Append valid UTF-8 without an intermediate allocation.
#[cfg(not(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128")))]
pub fn append_utf16(input: &str, output: &mut Vec<u16>) {
  #[cfg(all(feature = "simd", any(target_arch = "aarch64", target_arch = "x86_64")))]
  if input.len() >= 64 {
    return native::append_utf16(input, output);
  }
  output.extend(input.encode_utf16());
}

/// Match N-API's UTF-8 extraction, including replacement of lone JS surrogates.
#[cfg(not(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128")))]
pub fn from_utf16_lossy(input: &[u16]) -> String {
  #[cfg(all(feature = "simd", any(target_arch = "aarch64", target_arch = "x86_64")))]
  if input.len() >= 32 {
    return native::from_utf16_lossy(input);
  }
  scalar_from_utf16_lossy(input)
}

#[cfg(not(all(feature = "simd", target_arch = "wasm32", target_feature = "simd128")))]
fn scalar_from_utf16_lossy(input: &[u16]) -> String {
  let mut output = String::with_capacity(input.len().checked_mul(3).expect("input too large"));
  output.extend(
    char::decode_utf16(input.iter().copied()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)),
  );
  output
}
