//! Jieba word segmentation guiding the core pronunciation phrase selection.
//!
//! The caller owns the segmenter and may reuse or customize its word dictionary.
//! Jieba does not provide pinyin readings; unmatched characters keep core defaults.

use super::{Style, Tokens};
pub use jieba_rs::Jieba;

fn phrase_readings(input: &str, segmenter: &Jieba, hmm: bool) -> super::Prepared {
  let chars = super::decode(input);
  let mut choices = vec![0u16; chars.len()];
  let mut boundaries = vec![0u8; chars.len()];
  for word in segmenter.cut(input, hmm) {
    // Jieba's start/end are Unicode scalar offsets, not UTF-8 byte offsets.
    if let Some(boundary) = boundaries.get_mut(word.end) {
      *boundary = 1;
    }
  }
  super::resolve_phrase_readings::<true>(&chars, &mut choices, &boundaries);
  super::Prepared::new(chars, choices)
}

/// Prefer phrase matches inside Jieba words, retaining cross-boundary fallback.
/// Adjacent non-Han text remains grouped across segmentation boundaries.
pub fn tokens<'a>(input: &'a str, segmenter: &Jieba, hmm: bool) -> Tokens<'a> {
  if input.is_ascii() {
    return super::tokens(input, false);
  }
  super::tokens_with_overrides(input, phrase_readings(input, segmenter, hmm))
}

/// Append output while reusing the caller's segmenter and output capacity.
pub fn write_pinyin(
  input: &str,
  style: Style,
  segmenter: &Jieba,
  hmm: bool,
  separator: &str,
  output: &mut String,
) {
  if input.is_ascii() {
    output.push_str(input);
    return;
  }
  let overrides = phrase_readings(input, segmenter, hmm);
  if separator.is_empty() {
    super::write_characters::<false>(input, style, separator, overrides, output);
  } else {
    super::write_characters::<true>(input, style, separator, overrides, output);
  }
}

/// Return delimited pinyin using Jieba word boundaries and core phrase readings.
pub fn pinyin(input: &str, style: Style, segmenter: &Jieba, hmm: bool, separator: &str) -> String {
  let mut output = String::with_capacity(input.len().saturating_mul(2));
  write_pinyin(input, style, segmenter, hmm, separator, &mut output);
  output
}

/// Direct UTF-16 output using the same Jieba boundaries and phrase decisions.
#[cfg(feature = "utf16")]
pub fn pinyin_utf16(
  input: &str,
  style: Style,
  segmenter: &Jieba,
  hmm: bool,
  separator: &str,
) -> Vec<u16> {
  let mut output = Vec::with_capacity(input.len().saturating_mul(2));
  let separator: Vec<_> = separator.encode_utf16().collect();
  let overrides = if input.is_ascii() {
    super::Prepared::default()
  } else {
    phrase_readings(input, segmenter, hmm)
  };
  super::utf16::write_with_overrides(input, style, &separator, overrides, &mut output);
  output
}
