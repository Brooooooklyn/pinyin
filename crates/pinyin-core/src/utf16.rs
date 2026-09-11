//! Optional direct UTF-16 output for JavaScript and other two-byte string APIs.
//! Input and token ranges remain UTF-8; no runtime encoding cache is required.

use super::Style;

/// Append UTF-16, inserting separators only between tokens from this call.
pub fn write_pinyin(
  input: &str,
  style: Style,
  phrases: bool,
  separator: &[u16],
  output: &mut Vec<u16>,
) {
  let overrides = if phrases && !input.is_ascii() {
    super::phrase_readings(input)
  } else {
    super::Prepared::default()
  };
  write_with_overrides(input, style, separator, overrides, output);
}

/// Build delimited UTF-16 directly from precomputed syllables.
pub fn pinyin(input: &str, style: Style, phrases: bool, separator: &str) -> Vec<u16> {
  let mut output = Vec::with_capacity(input.len().saturating_mul(2));
  let separator: Vec<_> = separator.encode_utf16().collect();
  write_pinyin(input, style, phrases, &separator, &mut output);
  output
}

pub(super) fn write_with_overrides(
  input: &str,
  style: Style,
  separator: &[u16],
  overrides: super::Prepared,
  output: &mut Vec<u16>,
) {
  if input.is_ascii() {
    output.extend(input.bytes().map(u16::from));
  } else if separator.is_empty() {
    write_characters::<false>(input, style, separator, overrides, output);
  } else {
    write_characters::<true>(input, style, separator, overrides, output);
  }
}

#[inline]
fn write_characters<const SEPARATED: bool>(
  input: &str,
  style: Style,
  separator: &[u16],
  overrides: super::Prepared,
  output: &mut Vec<u16>,
) {
  fn append(input: &str, output: &mut Vec<u16>) {
    #[cfg(feature = "simd")]
    napi_pinyin_kernels::append_utf16(input, output);
    #[cfg(not(feature = "simd"))]
    output.extend(input.encode_utf16());
  }
  fn run<const SEPARATED: bool>(
    input: &str,
    style: Style,
    separator: &[u16],
    mut cursor: impl super::CharacterCursor,
    mut choices: std::vec::IntoIter<u16>,
    output: &mut Vec<u16>,
  ) {
    let table = &super::STYLES_UTF16[style as usize];
    let mut first = true;
    let mut unchanged = None;
    while let Some((i, ch)) = cursor.next() {
      let selected = choices.next().unwrap_or(0);
      let id = if selected != 0 {
        selected
      } else {
        super::entry(ch) as u16
      };
      if id != 0 {
        if let Some(start) = unchanged.take() {
          if SEPARATED && !first {
            output.extend_from_slice(separator);
          }
          append(&input[start..i], output);
          first = false;
        }
        if SEPARATED && !first {
          output.extend_from_slice(separator);
        }
        output.extend_from_slice(table[id as usize]);
        first = false;
      } else {
        unchanged.get_or_insert(i);
        if ch.is_ascii() {
          let skipped = cursor.skip_ascii();
          if skipped != 0 {
            choices.nth(skipped - 1);
          }
        }
      }
    }
    if let Some(start) = unchanged {
      if SEPARATED && !first {
        output.extend_from_slice(separator);
      }
      append(&input[start..], output);
    }
  }
  let choices = overrides.choices.into_iter();
  if overrides.chars.is_empty() {
    run::<SEPARATED>(
      input,
      style,
      separator,
      super::RawChars {
        chars: input.char_indices(),
        base: 0,
      },
      choices,
      output,
    );
  } else {
    run::<SEPARATED>(
      input,
      style,
      separator,
      super::DecodedChars {
        chars: overrides.chars.into_iter(),
        position: 0,
      },
      choices,
      output,
    );
  }
}
