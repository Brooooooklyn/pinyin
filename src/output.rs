//! Stream precomputed syllables straight into V8's UTF-16 representation.

use crate::encoding::append_utf16;
use pinyin_core::{Style, Token};

// Keep the escape kernel's control flow and register pressure out of the
// common dictionary-only JSON loop. Each call handles one unchanged token.
#[cfg(feature = "simd")]
#[inline(never)]
pub fn json_utf8_text(input: &str, output: &mut Vec<u8>) {
  json_escape_simd::escape_into(input, output);
}

#[cfg(not(feature = "simd"))]
pub fn json_utf8_text(input: &str, output: &mut Vec<u8>) {
  output.push(b'"');
  let mut start = 0;
  while let Some(relative) = crate::encoding::json_escape(&input.as_bytes()[start..]) {
    let i = start + relative;
    let byte = input.as_bytes()[i];
    output.extend_from_slice(&input.as_bytes()[start..i]);
    output.push(b'\\');
    match byte {
      b'"' | b'\\' => output.push(byte),
      b'\n' => output.push(b'n'),
      b'\r' => output.push(b'r'),
      b'\t' => output.push(b't'),
      _ => {
        const HEX: &[u8] = b"0123456789abcdef";
        output.extend_from_slice(b"u00");
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 15) as usize]);
      }
    }
    start = i + 1;
  }
  output.extend_from_slice(&input.as_bytes()[start..]);
  output.push(b'"');
}

pub fn json_utf16(
  input: &str,
  tokens: impl Iterator<Item = Token>,
  style: Style,
  multi: bool,
) -> Vec<u16> {
  let mut output = Vec::with_capacity(input.len().saturating_mul(2));
  output.push(b'[' as u16);
  for (i, token) in tokens.enumerate() {
    if i != 0 {
      output.push(b',' as u16);
    }
    if multi {
      output.push(b'[' as u16);
    }
    output.push(b'"' as u16);
    if let Some(syllable) = token.syllable() {
      if multi {
        for (j, syllable) in token.readings().enumerate() {
          if j != 0 {
            output.extend_from_slice(&[b'"' as u16, b',' as u16, b'"' as u16]);
          }
          output.extend_from_slice(syllable.utf16(style));
        }
      } else {
        output.extend_from_slice(syllable.utf16(style));
      }
    } else {
      json_text(token.text(input, style), &mut output);
    }
    output.push(b'"' as u16);
    if multi {
      output.push(b']' as u16);
    }
  }
  output.push(b']' as u16);
  output
}

fn json_text(input: &str, output: &mut Vec<u16>) {
  let mut start = 0;
  while let Some(relative) = crate::encoding::json_escape(&input.as_bytes()[start..]) {
    let i = start + relative;
    let byte = input.as_bytes()[i];
    // ASCII escape bytes always lie on UTF-8 boundaries. Copy intervening
    // runs in bulk so the transcoder can vectorize long unchanged text.
    append_utf16(&input[start..i], output);
    output.push(b'\\' as u16);
    let escaped = match byte {
      b'"' | b'\\' => byte,
      b'\n' => b'n',
      b'\r' => b'r',
      b'\t' => b't',
      _ => {
        const HEX: &[u8] = b"0123456789abcdef";
        output.extend_from_slice(&[b'u' as u16, b'0' as u16, b'0' as u16]);
        output.push(HEX[(byte >> 4) as usize] as u16);
        HEX[(byte & 15) as usize]
      }
    };
    output.push(escaped as u16);
    start = i + 1;
  }
  append_utf16(&input[start..], output);
}
