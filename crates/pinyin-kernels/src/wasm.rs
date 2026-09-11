//! wasm SIMD128: ASCII and three-byte BMP blocks, with scalar Unicode tails.
use std::arch::wasm32::*;

// Failed block probes cost more than scalar decoding on mixed input. Sample
// both ends, then retain scalar fallback inside the vector loop for arbitrary
// Unicode in the middle. This affects performance only, never validity.
fn dense_utf16(input: &[u16]) -> bool {
  input.len() >= 128
    && [&input[..32], &input[input.len() - 32..]]
      .iter()
      .all(|part| {
        part.iter().filter(|&&c| c < 128).count() >= 31
          || part
            .iter()
            .filter(|&&c| c >= 0x800 && !(0xd800..0xe000).contains(&c))
            .count()
            >= 28
      })
}

fn dense_utf8(input: &str) -> bool {
  fn dense(chars: impl Iterator<Item = char>) -> bool {
    let mut ascii = 0;
    let mut bmp = 0;
    for ch in chars.take(32) {
      ascii += usize::from(ch.is_ascii());
      bmp += usize::from(ch.len_utf8() == 3);
    }
    ascii >= 31 || bmp >= 28
  }
  input.len() >= 512 && dense(input.chars()) && dense(input.chars().rev())
}

pub fn append_utf16(input: &str, output: &mut Vec<u16>) {
  if !dense_utf8(input) {
    output.extend(input.encode_utf16());
    return;
  }
  output.reserve(input.len());
  let mut i = 0;
  while i < input.len() {
    if input.len() - i >= 16 {
      // SAFETY: a complete input block is available; reserve bounds all output
      // stores. Input is valid UTF-8; only complete ASCII/BMP groups are used.
      unsafe {
        let v = v128_load(input.as_ptr().add(i).cast());
        let dst = output.as_mut_ptr().add(output.len());
        if i8x16_bitmask(v) == 0 {
          v128_store(dst.cast(), u16x8_extend_low_u8x16(v));
          v128_store(dst.add(8).cast(), u16x8_extend_high_u8x16(v));
          output.set_len(output.len() + 16);
          i += 16;
          continue;
        }
        let a = i8x16_swizzle(
          v,
          u8x16(0, 3, 6, 9, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16),
        );
        let b = i8x16_swizzle(
          v,
          u8x16(1, 4, 7, 10, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16),
        );
        let c = i8x16_swizzle(
          v,
          u8x16(2, 5, 8, 11, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16),
        );
        let valid = v128_and(
          u8x16_eq(v128_and(a, u8x16_splat(0xf0)), u8x16_splat(0xe0)),
          v128_and(
            u8x16_eq(v128_and(b, u8x16_splat(0xc0)), u8x16_splat(0x80)),
            u8x16_eq(v128_and(c, u8x16_splat(0xc0)), u8x16_splat(0x80)),
          ),
        );
        if i8x16_bitmask(valid) & 15 == 15 {
          let a = u16x8_shl(u16x8_extend_low_u8x16(v128_and(a, u8x16_splat(15))), 12);
          let b = u16x8_shl(u16x8_extend_low_u8x16(v128_and(b, u8x16_splat(63))), 6);
          let c = u16x8_extend_low_u8x16(v128_and(c, u8x16_splat(63)));
          v128_store64_lane::<0>(v128_or(a, v128_or(b, c)), dst.cast());
          output.set_len(output.len() + 4);
          i += 12;
          continue;
        }
      }
    }
    let ch = input[i..].chars().next().unwrap();
    output.extend_from_slice(ch.encode_utf16(&mut [0; 2]));
    i += ch.len_utf8();
  }
}

pub fn from_utf16_lossy(input: &[u16]) -> String {
  if !dense_utf16(input) {
    let mut output = String::with_capacity(input.len().checked_mul(3).expect("input too large"));
    output.extend(
      char::decode_utf16(input.iter().copied()).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)),
    );
    return output;
  }
  let mut output = Vec::<u8>::with_capacity(input.len().checked_mul(3).expect("input too large"));
  let mut i = 0;
  while i < input.len() {
    if input.len() - i >= 8 {
      // SAFETY: input contains eight units. ASCII writes eight bytes; the BMP
      // path excludes surrogates and writes exactly 24 bytes. Capacity is 3N.
      unsafe {
        let v = v128_load(input.as_ptr().add(i).cast());
        let dst = output.as_mut_ptr().add(output.len());
        if i16x8_all_true(u16x8_lt(v, u16x8_splat(128))) {
          v128_store64_lane::<0>(u8x16_narrow_i16x8(v, v), dst.cast());
          output.set_len(output.len() + 8);
          i += 8;
          continue;
        }
        let bmp = v128_and(
          u16x8_ge(v, u16x8_splat(0x800)),
          v128_or(
            u16x8_lt(v, u16x8_splat(0xd800)),
            u16x8_ge(v, u16x8_splat(0xe000)),
          ),
        );
        if i16x8_all_true(bmp) {
          let a = v128_or(u16x8_shr(v, 12), u16x8_splat(0xe0));
          let b = v128_or(
            v128_and(u16x8_shr(v, 6), u16x8_splat(63)),
            u16x8_splat(0x80),
          );
          let c = v128_or(v128_and(v, u16x8_splat(63)), u16x8_splat(0x80));
          let a = u8x16_narrow_i16x8(a, a);
          let b = u8x16_narrow_i16x8(b, b);
          let c = u8x16_narrow_i16x8(c, c);
          let first = v128_or(
            i8x16_swizzle(
              a,
              u8x16(0, 16, 16, 1, 16, 16, 2, 16, 16, 3, 16, 16, 4, 16, 16, 5),
            ),
            v128_or(
              i8x16_swizzle(
                b,
                u8x16(16, 0, 16, 16, 1, 16, 16, 2, 16, 16, 3, 16, 16, 4, 16, 16),
              ),
              i8x16_swizzle(
                c,
                u8x16(16, 16, 0, 16, 16, 1, 16, 16, 2, 16, 16, 3, 16, 16, 4, 16),
              ),
            ),
          );
          let last = v128_or(
            i8x16_swizzle(
              a,
              u8x16(16, 16, 6, 16, 16, 7, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16),
            ),
            v128_or(
              i8x16_swizzle(
                b,
                u8x16(5, 16, 16, 6, 16, 16, 7, 16, 16, 16, 16, 16, 16, 16, 16, 16),
              ),
              i8x16_swizzle(
                c,
                u8x16(16, 5, 16, 16, 6, 16, 16, 7, 16, 16, 16, 16, 16, 16, 16, 16),
              ),
            ),
          );
          v128_store(dst.cast(), first);
          v128_store64_lane::<0>(last, dst.add(16).cast());
          output.set_len(output.len() + 24);
          i += 8;
          continue;
        }
      }
    }
    let decoded = char::decode_utf16(input[i..].iter().copied())
      .next()
      .unwrap();
    let valid = decoded.is_ok();
    let ch = decoded.unwrap_or(char::REPLACEMENT_CHARACTER);
    i += if valid { ch.len_utf16() } else { 1 };
    output.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
  }
  // SAFETY: SIMD emits valid ASCII or non-surrogate BMP encodings; the scalar
  // tail uses standard lossy decoding and char::encode_utf8.
  unsafe { String::from_utf8_unchecked(output) }
}
