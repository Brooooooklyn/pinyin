//! Rust-only SIMD transcoding. Every block load/store is bounded; mixed Unicode
//! and malformed UTF-16 use the standard scalar codecs at character boundaries.
#[cfg(target_arch = "aarch64")]
#[path = "neon.rs"]
mod blocks;
#[cfg(target_arch = "x86_64")]
#[path = "ssse3.rs"]
mod blocks;

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

pub fn append_utf16(input: &str, output: &mut Vec<u16>) {
  if !available() {
    output.extend(input.encode_utf16());
    return;
  }
  output.reserve(input.len());
  let mut i = 0;
  while i < input.len() {
    // SAFETY: CPU support was checked; block checks its input length. The UTF-8
    // byte count bounds UTF-16 output, so all writes fit the reserved capacity.
    let (read, written) = unsafe {
      blocks::utf8(
        &input.as_bytes()[i..],
        output.as_mut_ptr().add(output.len()),
      )
    };
    if read != 0 {
      // SAFETY: the block initialized exactly `written` units.
      unsafe {
        output.set_len(output.len() + written);
      }
      i += read;
    } else {
      let ch = input[i..].chars().next().unwrap();
      output.extend_from_slice(ch.encode_utf16(&mut [0; 2]));
      i += ch.len_utf8();
    }
  }
}

pub fn decode(input: &str) -> Vec<char> {
  if !available() {
    return input.chars().collect();
  }
  let mut output = Vec::with_capacity(input.len() / 3);
  let mut i = 0;
  while i < input.len() {
    let mut units = [0u16; 16];
    // SAFETY: CPU support was checked. A block writes at most 16 units and
    // accepts only ASCII or complete three-byte encodings from valid UTF-8.
    let (read, written) = unsafe { blocks::utf8(&input.as_bytes()[i..], units.as_mut_ptr()) };
    if read != 0 {
      output.extend(units[..written].iter().map(|&unit| {
        // SAFETY: the block emits only ASCII or non-surrogate BMP scalars.
        unsafe { char::from_u32_unchecked(u32::from(unit)) }
      }));
      i += read;
    } else {
      let ch = input[i..].chars().next().unwrap();
      output.push(ch);
      i += ch.len_utf8();
    }
  }
  output
}

pub fn from_utf16_lossy(input: &[u16]) -> String {
  if !available() {
    return super::scalar_from_utf16_lossy(input);
  }
  let mut output = Vec::<u8>::with_capacity(input.len().checked_mul(3).expect("input too large"));
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
      let (read, bytes) = blocks::utf16(&input[i..], dst);
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
            .unwrap();
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
    String::from_utf8_unchecked(output)
  }
}
