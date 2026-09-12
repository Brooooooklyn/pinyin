//! Rust-only SIMD transcoding with bounded block loads and stores.
//! Mixed Unicode and malformed UTF-16 use scalar codecs at character boundaries.

use crate::{
  error::{next_utf8, reserve, utf8_capacity},
  Error, Result,
};
#[cfg(target_arch = "aarch64")]
#[path = "neon.rs"]
mod blocks;
#[cfg(target_arch = "x86_64")]
#[path = "ssse3.rs"]
mod blocks;
#[cfg(target_arch = "x86_64")]
#[path = "avx2.rs"]
mod avx2;
#[cfg(target_arch = "x86_64")]
#[path = "avx512.rs"]
mod avx512;

fn available() -> bool {
  #[cfg(target_arch = "aarch64")]
  {
    true
  }
  #[cfg(target_arch = "x86_64")]
  {
    std::is_x86_feature_detected!("ssse3")
  }
}

/// Widest x86 block tier supported by the running CPU. SSSE3 is the floor
/// because `available` gates the whole module on it.
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Tier {
  Ssse3,
  Avx2,
  Avx512,
}

#[cfg(target_arch = "x86_64")]
fn tier() -> Tier {
  // Probe once per process; the detect macros are atomic loads afterwards,
  // and the OnceLock keeps repeated calls to a single predictable read.
  static TIER: std::sync::OnceLock<Tier> = std::sync::OnceLock::new();
  *TIER.get_or_init(|| {
    if std::is_x86_feature_detected!("avx512f")
      && std::is_x86_feature_detected!("avx512bw")
      && std::is_x86_feature_detected!("avx512vl")
      && std::is_x86_feature_detected!("avx512vbmi")
      && std::is_x86_feature_detected!("avx2")
      && std::is_x86_feature_detected!("ssse3")
    {
      Tier::Avx512
    } else if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("ssse3") {
      Tier::Avx2
    } else {
      Tier::Ssse3
    }
  })
}

/// Widest-block first; a gate failure retries narrower tiers so mixed text
/// keeps SIMD coverage at SSSE3 widths instead of falling to scalar.
#[cfg(target_arch = "x86_64")]
#[inline]
fn utf8_block(input: &[u8], dst: *mut u16) -> (usize, usize) {
  // SAFETY: every tier was detected at runtime; blocks check their own input
  // length and write only the returned units.
  unsafe {
    let (read, written) = match tier() {
      Tier::Avx512 => avx512::utf8(input, dst),
      Tier::Avx2 => avx2::utf8(input, dst),
      Tier::Ssse3 => blocks::utf8(input, dst),
    };
    if read != 0 {
      return (read, written);
    }
    match tier() {
      Tier::Avx512 => {
        let (read, written) = avx2::utf8(input, dst);
        if read != 0 {
          (read, written)
        } else {
          blocks::utf8(input, dst)
        }
      }
      Tier::Avx2 => blocks::utf8(input, dst),
      Tier::Ssse3 => (0, 0),
    }
  }
}

/// Same cascade for the UTF-16 to UTF-8 direction.
#[cfg(target_arch = "x86_64")]
#[inline]
fn utf16_block(input: &[u16], dst: *mut u8) -> (usize, usize) {
  // SAFETY: every tier was detected at runtime; blocks check their own input
  // length and write only the returned bytes.
  unsafe {
    let (read, written) = match tier() {
      Tier::Avx512 => avx512::utf16(input, dst),
      Tier::Avx2 => avx2::utf16(input, dst),
      Tier::Ssse3 => blocks::utf16(input, dst),
    };
    if read != 0 {
      return (read, written);
    }
    match tier() {
      Tier::Avx512 => {
        let (read, written) = avx2::utf16(input, dst);
        if read != 0 {
          (read, written)
        } else {
          blocks::utf16(input, dst)
        }
      }
      Tier::Avx2 => blocks::utf16(input, dst),
      Tier::Ssse3 => (0, 0),
    }
  }
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn utf8_block(input: &[u8], dst: *mut u16) -> (usize, usize) {
  // SAFETY: NEON is baseline; the block checks its input length.
  unsafe { blocks::utf8(input, dst) }
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn utf16_block(input: &[u16], dst: *mut u8) -> (usize, usize) {
  // SAFETY: NEON is baseline; the block checks its input length.
  unsafe { blocks::utf16(input, dst) }
}

pub fn append_utf16(input: &str, output: &mut Vec<u16>) -> Result<()> {
  append_utf16_impl(input, output, utf8_block)
}

fn append_utf16_impl(
  input: &str,
  output: &mut Vec<u16>,
  block: impl Fn(&[u8], *mut u16) -> (usize, usize),
) -> Result<()> {
  reserve(output, input.len(), "UTF-8 to UTF-16")?;
  if !available() {
    output.extend(input.encode_utf16());
    return Ok(());
  }
  let mut i = 0;
  while i < input.len() {
    // SAFETY: CPU support was checked; block checks its input length. The UTF-8
    // byte count bounds UTF-16 output, so all writes fit the reserved capacity.
    let dst = unsafe { output.as_mut_ptr().add(output.len()) };
    let (read, written) = block(&input.as_bytes()[i..], dst);
    if read != 0 {
      // SAFETY: the block initialized exactly `written` units.
      unsafe {
        output.set_len(output.len() + written);
      }
      i += read;
    } else {
      let ch = next_utf8(input, i)?;
      output.extend_from_slice(ch.encode_utf16(&mut [0; 2]));
      i += ch.len_utf8();
    }
  }
  Ok(())
}

pub fn decode(input: &str) -> Result<Vec<char>> {
  decode_impl(input, utf8_block)
}

fn decode_impl(
  input: &str,
  block: impl Fn(&[u8], *mut u16) -> (usize, usize),
) -> Result<Vec<char>> {
  if !available() {
    return Ok(input.chars().collect());
  }
  let mut output = Vec::new();
  reserve(&mut output, input.len() / 3, "UTF-8 character decoding")?;
  let mut i = 0;
  while i < input.len() {
    let mut units = [0u16; 64];
    // SAFETY: CPU support was checked. A block writes at most 64 units and
    // accepts only ASCII or complete three-byte encodings from valid UTF-8.
    let (read, written) = block(&input.as_bytes()[i..], units.as_mut_ptr());
    if read != 0 {
      reserve(&mut output, written, "UTF-8 character decoding")?;
      output.extend(units[..written].iter().map(|&unit| {
        // SAFETY: the block emits only ASCII or non-surrogate BMP scalars.
        unsafe { char::from_u32_unchecked(u32::from(unit)) }
      }));
      i += read;
    } else {
      let ch = next_utf8(input, i)?;
      reserve(&mut output, 1, "UTF-8 character decoding")?;
      output.push(ch);
      i += ch.len_utf8();
    }
  }
  Ok(output)
}

pub fn from_utf16_lossy(input: &[u16]) -> Result<String> {
  from_utf16_lossy_impl(input, utf16_block)
}

fn from_utf16_lossy_impl(
  input: &[u16],
  block: impl Fn(&[u16], *mut u8) -> (usize, usize),
) -> Result<String> {
  if !available() {
    return super::scalar_from_utf16_lossy(input);
  }
  let mut output = Vec::<u8>::new();
  reserve(&mut output, utf8_capacity(input.len())?, "UTF-16 to UTF-8")?;
  let mut i = 0;
  let mut written = 0;
  // SAFETY: each input unit produces at most three bytes (a valid surrogate
  // pair produces four for two units). Capacity is 3N and input is independent
  // of the output allocation. Blocks check input bounds and CPU support was
  // checked above. The scalar path encodes BMP directly, validates surrogate
  // pairs with the standard decoder, and replaces individual invalid units.
  // Only the completely initialized UTF-8 prefix is exposed at the end.
  unsafe {
    while i < input.len() {
      let dst = output.as_mut_ptr().add(written);
      let (read, bytes) = block(&input[i..], dst);
      if read != 0 {
        i += read;
        written += bytes;
        continue;
      }
      let end = input.len().min(i + 8);
      while i < end {
        let unit = input[i];
        let dst = output.as_mut_ptr().add(written);
        if unit < 128 {
          dst.write(unit as u8);
          written += 1;
          i += 1;
        } else if unit < 0x800 {
          dst.write(0xc0 | (unit >> 6) as u8);
          dst.add(1).write(0x80 | (unit & 63) as u8);
          written += 2;
          i += 1;
        } else if !(0xd800..0xe000).contains(&unit) {
          dst.write(0xe0 | (unit >> 12) as u8);
          dst.add(1).write(0x80 | ((unit >> 6) & 63) as u8);
          dst.add(2).write(0x80 | (unit & 63) as u8);
          written += 3;
          i += 1;
        } else {
          let decoded = char::decode_utf16(input[i..].iter().copied())
            .next()
            .ok_or(Error::InvalidCursor {
              encoding: "UTF-16",
              offset: i,
              input_len: input.len(),
            })?;
          let valid = decoded.is_ok();
          let ch = decoded.unwrap_or(char::REPLACEMENT_CHARACTER);
          i += if valid { ch.len_utf16() } else { 1 };
          let mut bytes = [0; 4];
          let encoded = ch.encode_utf8(&mut bytes);
          std::ptr::copy_nonoverlapping(encoded.as_ptr(), dst, encoded.len());
          written += encoded.len();
        }
      }
    }
    output.set_len(written);
    Ok(String::from_utf8_unchecked(output))
  }
}

#[cfg(all(test, target_arch = "x86_64"))]
mod tier_tests {
  use super::*;

  fn corpus() -> Vec<String> {
    let scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    let mut corpus = vec![
      String::new(),
      "é".repeat(40),       // two-byte only: exercises the scalar lane
      "aé中🙂".repeat(40),   // mixed widths, forces tier cascades
      "中\0a🙂é国".repeat(30),
      scalars.clone(),
      format!("中中中中{scalars}国国国国"),
    ];
    for n in [1usize, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 128, 500] {
      corpus.push("a".repeat(n));
      corpus.push("中".repeat(n));
      corpus.push("a中".repeat(n));
      corpus.push(format!("{}中{}", "a".repeat(n), "文".repeat(n)));
    }
    corpus
  }

  #[test]
  fn every_detected_tier_matches_std_utf8_to_utf16() {
    if !std::is_x86_feature_detected!("ssse3") {
      return;
    }
    let mut tiers: Vec<(&str, unsafe fn(&[u8], *mut u16) -> (usize, usize))> =
      vec![("ssse3", blocks::utf8)];
    if std::is_x86_feature_detected!("avx2") {
      tiers.push(("avx2", avx2::utf8));
    }
    if std::is_x86_feature_detected!("avx512f")
      && std::is_x86_feature_detected!("avx512bw")
      && std::is_x86_feature_detected!("avx512vl")
      && std::is_x86_feature_detected!("avx512vbmi")
    {
      tiers.push(("avx512", avx512::utf8));
    }
    for (name, f) in tiers {
      for text in corpus() {
        let mut out = Vec::new();
        append_utf16_impl(&text, &mut out, |input, dst| unsafe { f(input, dst) }).unwrap();
        let expected: Vec<u16> = text.encode_utf16().collect();
        assert_eq!(out, expected, "{name} utf8: len {}", text.len());
        let chars = decode_impl(&text, |input, dst| unsafe { f(input, dst) }).unwrap();
        assert_eq!(chars, text.chars().collect::<Vec<_>>(), "{name} decode: len {}", text.len());
      }
    }
  }

  #[test]
  fn every_detected_tier_matches_std_utf16_to_utf8() {
    if !std::is_x86_feature_detected!("ssse3") {
      return;
    }
    let mut tiers: Vec<(&str, unsafe fn(&[u16], *mut u8) -> (usize, usize))> =
      vec![("ssse3", blocks::utf16)];
    if std::is_x86_feature_detected!("avx2") {
      tiers.push(("avx2", avx2::utf16));
    }
    if std::is_x86_feature_detected!("avx512f")
      && std::is_x86_feature_detected!("avx512bw")
      && std::is_x86_feature_detected!("avx512vl")
      && std::is_x86_feature_detected!("avx512vbmi")
    {
      tiers.push(("avx512", avx512::utf16));
    }
    for (name, f) in tiers {
      for text in corpus() {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        let out = from_utf16_lossy_impl(&units, |input, dst| unsafe { f(input, dst) }).unwrap();
        assert_eq!(out, text, "{name} utf16: len {}", text.len());
        // Lone surrogates at start, middle and end must degrade identically.
        for position in [0, units.len() / 2, units.len()] {
          for lone in [0xd800u16, 0xdc00] {
            units.insert(position, lone);
            let out = from_utf16_lossy_impl(&units, |input, dst| unsafe { f(input, dst) }).unwrap();
            assert_eq!(out, String::from_utf16_lossy(&units), "{name} utf16 lone {lone:#x} at {position}");
            units.remove(position);
          }
        }
      }
    }
  }
}
