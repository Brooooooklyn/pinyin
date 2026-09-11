//! Jieba word segmentation guiding the core pronunciation phrase selection.
//!
//! The caller owns the segmenter and may reuse or customize its word dictionary.
//! Jieba does not provide pinyin readings; unmatched characters keep core defaults.

use super::{Style, Tokens};
pub use jieba_rs::Jieba;

/// Compatibility iterator for the original Node binding's `segment: true`.
/// Readings remain per-character. A word containing any mapped character
/// discards its unmapped characters; wholly unmapped words are grouped.
pub struct LegacyTokens<'a> {
  words: std::vec::IntoIter<jieba_rs::Token<'a>>,
  chars: std::str::CharIndices<'a>,
  offset: usize,
  base: usize,
  unchanged: Option<super::Token>,
}

pub fn legacy_tokens<'a>(input: &'a str, segmenter: &Jieba) -> LegacyTokens<'a> {
  let ascii = input.is_ascii();
  LegacyTokens {
    words: if ascii {
      Vec::new()
    } else {
      segmenter.cut(input, false)
    }
    .into_iter(),
    chars: "".char_indices(),
    offset: 0,
    base: 0,
    unchanged: (ascii && !input.is_empty()).then_some(super::Token {
      start: 0,
      end: input.len(),
      entry: 0,
    }),
  }
}

impl Iterator for LegacyTokens<'_> {
  type Item = super::Token;

  fn next(&mut self) -> Option<Self::Item> {
    loop {
      for (offset, ch) in self.chars.by_ref() {
        let entry = super::entry(ch);
        if entry != 0 {
          let start = self.base + offset;
          return Some(super::Token {
            start,
            end: start + ch.len_utf8(),
            entry,
          });
        }
      }
      let Some(word) = self.words.next() else {
        return self.unchanged.take();
      };
      self.base = self.offset;
      self.offset += word.word.len();
      if word.word.chars().any(|ch| super::entry(ch) != 0) {
        self.chars = word.word.char_indices();
        if let Some(unchanged) = self.unchanged.take() {
          return Some(unchanged);
        }
      } else if let Some(unchanged) = &mut self.unchanged {
        unchanged.end = self.offset;
      } else {
        self.unchanged = Some(super::Token {
          start: self.base,
          end: self.offset,
          entry: 0,
        });
      }
    }
  }
}

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
