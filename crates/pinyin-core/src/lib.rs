//! Static Chinese pronunciation tables and phrase disambiguation.
//!
//! Character conversion preserves the reading order of pinyin-data 0.15.0.
//! Contiguous characters without a reading are returned as one borrowed token.
//! Phrase conversion uses a precompiled trie and a bounded dynamic program.
//! No dictionary construction, hashing, locks, or worker threads run at startup.
#![forbid(unsafe_code)]

use std::{cmp::Ordering, str::CharIndices};

/// Optional Jieba word boundaries combined with the core pronunciation dictionary.
#[cfg(feature = "jieba")]
pub mod jieba;

#[cfg(feature = "utf16")]
pub mod utf16;

struct Node {
  start: u32,
  len: u16,
  phrase: u16,
}

include!(concat!(env!("OUT_DIR"), "/dictionary.rs"));

/// Syllable format. Numeric styles leave neutral tones unnumbered.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
  #[default]
  Plain,
  Tone,
  ToneNumber,
  ToneNumberEnd,
  FirstLetter,
}

/// A dictionary syllable. IDs are private and may change with dictionary updates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Syllable(u16);

impl Syllable {
  #[inline]
  pub fn text(self, style: Style) -> &'static str {
    STYLES[style as usize][self.0 as usize]
  }

  /// Precomputed UTF-16 for engines using two-byte strings. Enabled by `utf16`.
  #[cfg(feature = "utf16")]
  #[inline]
  pub fn utf16(self, style: Style) -> &'static [u16] {
    STYLES_UTF16[style as usize][self.0 as usize]
  }

  /// Index for a call-local cache; valid only for this build of the dictionary.
  #[inline]
  pub fn cache_index(self) -> usize {
    self.0 as usize
  }
}

#[inline]
fn entry(ch: char) -> u32 {
  let code = ch as usize;
  let common = code.wrapping_sub(0x4e00);
  if common < COMMON.len() {
    COMMON[common]
  } else {
    CHAR_DATA[usize::from(CHAR_PAGES[code >> 8]) * 256 + (code & 255)]
  }
}

/// First dictionary pronunciation, or `None` for an unmapped Unicode scalar.
#[inline]
pub fn lookup(ch: char) -> Option<Syllable> {
  let id = entry(ch) as u16;
  (id != 0).then_some(Syllable(id))
}

/// All readings, in the source dictionary's stable order (including tone variants).
pub fn readings(ch: char) -> impl ExactSizeIterator<Item = Syllable> + Clone {
  READINGS[(entry(ch) >> 16) as usize]
    .iter()
    .copied()
    .map(Syllable)
}

/// A pronunciation or an unchanged byte range in the original input.
#[derive(Clone, Copy, Debug)]
pub struct Token {
  start: usize,
  end: usize,
  entry: u32,
}

impl Token {
  #[inline]
  pub fn syllable(self) -> Option<Syllable> {
    (self.entry != 0).then_some(Syllable(self.entry as u16))
  }

  /// The range always lies on UTF-8 boundaries in the input passed to `tokens`.
  pub fn range(self) -> std::ops::Range<usize> {
    self.start..self.end
  }

  /// Render using the same input passed to `tokens`.
  #[inline]
  pub fn text(self, input: &str, style: Style) -> &str {
    match self.syllable() {
      Some(syllable) => syllable.text(style),
      None => &input[self.start..self.end],
    }
  }

  pub fn readings(self) -> impl ExactSizeIterator<Item = Syllable> + Clone {
    READINGS[(self.entry >> 16) as usize]
      .iter()
      .copied()
      .map(Syllable)
  }
}

#[derive(Default)]
struct Prepared {
  chars: Vec<char>,
  choices: Vec<u16>,
}

impl Prepared {
  fn new(chars: Vec<char>, choices: Vec<u16>) -> Self {
    // Reuse pays on dense BMP text. ASCII-heavy output is better served by
    // the byte cursor, which can skip whole spans instead of walking u32s.
    let reuse = chars.len() >= 128
      && chars[..32].iter().filter(|c| c.len_utf8() == 3).count() >= 28
      && chars[chars.len() - 32..]
        .iter()
        .filter(|c| c.len_utf8() == 3)
        .count()
        >= 28;
    Self {
      chars: if reuse { chars } else { Vec::new() },
      choices,
    }
  }
}

/// A lazy stream. Character mode does not allocate; phrase mode stores O(n)
/// compact choices and uses O(maximum phrase length) probability scratch space.
pub struct Tokens<'a> {
  chars: CharIndices<'a>,
  pending: Option<Token>,
  overrides: std::vec::IntoIter<u16>,
  ascii: Option<Token>,
}

trait CharacterCursor: Iterator<Item = (usize, char)> {
  /// Skip ASCII immediately following a character already consumed.
  fn skip_ascii(&mut self) -> usize;
}

struct RawChars<'a> {
  chars: CharIndices<'a>,
  base: usize,
}
impl Iterator for RawChars<'_> {
  type Item = (usize, char);
  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    self.chars.next().map(|(i, c)| (self.base + i, c))
  }
}
impl CharacterCursor for RawChars<'_> {
  #[inline]
  fn skip_ascii(&mut self) -> usize {
    let rest = self.chars.as_str();
    let count = ascii_prefix(rest.as_bytes());
    if count != 0 {
      self.base += self.chars.offset() + count;
      self.chars = rest[count..].char_indices();
    }
    count
  }
}

struct DecodedChars {
  chars: std::vec::IntoIter<char>,
  position: usize,
}
impl Iterator for DecodedChars {
  type Item = (usize, char);
  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    let ch = self.chars.next()?;
    let start = self.position;
    self.position += ch.len_utf8();
    Some((start, ch))
  }
}
impl CharacterCursor for DecodedChars {
  #[inline]
  fn skip_ascii(&mut self) -> usize {
    let chars = self.chars.as_slice();
    let mut count = 0;
    while chars.len() - count >= 16 && chars[count..count + 16].iter().all(char::is_ascii) {
      count += 16;
    }
    count += chars[count..]
      .iter()
      .position(|c| !c.is_ascii())
      .unwrap_or(chars.len() - count);
    if count != 0 {
      self.chars.nth(count - 1);
      self.position += count;
    }
    count
  }
}

/// Construct a token stream. Contextual modes retain compact phrase choices.
pub fn tokens(input: &str, phrases: bool) -> Tokens<'_> {
  let prepared = if phrases && !input.is_ascii() {
    phrase_readings(input)
  } else {
    Prepared::default()
  };
  tokens_with_overrides(input, prepared)
}

#[inline]
fn ascii_prefix(input: &[u8]) -> usize {
  #[cfg(feature = "simd")]
  {
    napi_pinyin_kernels::ascii_prefix(input)
  }
  #[cfg(not(feature = "simd"))]
  {
    let mut i = 0;
    while input.len() - i >= 32 && input[i..i + 32].is_ascii() {
      i += 32;
    }
    i + input[i..]
      .iter()
      .position(|b| !b.is_ascii())
      .unwrap_or(input.len() - i)
  }
}

fn decode(input: &str) -> Vec<char> {
  #[cfg(feature = "simd")]
  {
    napi_pinyin_kernels::decode(input)
  }
  #[cfg(not(feature = "simd"))]
  {
    input.chars().collect()
  }
}

fn tokens_with_overrides(input: &str, prepared: Prepared) -> Tokens<'_> {
  let ascii = input.is_ascii();
  Tokens {
    chars: if ascii {
      "".char_indices()
    } else {
      input.char_indices()
    },
    pending: None,
    overrides: prepared.choices.into_iter(),
    ascii: (ascii && !input.is_empty()).then_some(Token {
      start: 0,
      end: input.len(),
      entry: 0,
    }),
  }
}

impl Tokens<'_> {
  #[inline]
  fn next_char(&mut self) -> Option<Token> {
    let (start, ch) = self.chars.next()?;
    let mut entry = entry(ch);
    if let Some(id) = self.overrides.next() {
      if id != 0 {
        entry = (entry & 0xffff0000) | u32::from(id);
      }
    }
    Some(Token {
      start,
      end: start + ch.len_utf8(),
      entry,
    })
  }
}

impl Iterator for Tokens<'_> {
  type Item = Token;

  #[inline]
  fn next(&mut self) -> Option<Token> {
    if let Some(ascii) = self.ascii.take() {
      return Some(ascii);
    }
    let mut token = self.pending.take().or_else(|| self.next_char())?;
    if token.entry != 0 {
      return Some(token);
    }
    while let Some(next) = self.next_char() {
      if next.entry != 0 {
        self.pending = Some(next);
        break;
      }
      token.end = next.end;
    }
    Some(token)
  }
}

#[inline]
fn root(ch: char) -> usize {
  let code = ch as usize;
  ROOT_DATA[usize::from(ROOT_PAGES[code >> 8]) * 256 + (code & 255)] as usize
}

#[inline]
fn child(state: usize, ch: char) -> Option<usize> {
  let node = &NODES[state];
  // SIMD builds pack a side-table index above the 16-bit edge offset, keeping
  // Node at eight bytes. Only nodes with 2-4 children get a padded label block.
  #[cfg(feature = "simd")]
  let start = (node.start & 0xffff) as usize;
  #[cfg(not(feature = "simd"))]
  let start = node.start as usize;
  let edges = &EDGES[start..start + usize::from(node.len)];
  #[cfg(feature = "simd")]
  if (2..=4).contains(&node.len) {
    return napi_pinyin_kernels::find4(&SMALL_LABELS[(node.start >> 16) as usize], ch as u32)
      .filter(|&lane| lane < usize::from(node.len))
      .map(|lane| edges[lane].1 as usize);
  }
  let code = ch as u32;
  // Almost every non-root node has few edges. A short linear scan avoids the
  // branch/dependency chain of binary search; large fanouts stay logarithmic.
  if edges.len() <= 4 {
    edges
      .iter()
      .find(|edge| edge.0 == code)
      .map(|edge| edge.1 as usize)
  } else {
    edges
      .binary_search_by_key(&code, |edge| edge.0)
      .ok()
      .map(|index| edges[index].1 as usize)
  }
}

fn phrase_readings(input: &str) -> Prepared {
  let chars = decode(input);
  let mut choices = vec![0u16; chars.len()];
  resolve_phrase_readings::<false>(&chars, &mut choices, &[]);
  Prepared::new(chars, choices)
}

// The caller supplies zeroed choices. A boundary at index i lies before chars[i].
// A soft penalty preserves useful phrases such as 分为 when Jieba cuts 划分/为.
// The unsegmented specialization compiles out boundary processing entirely.
fn resolve_phrase_readings<const JIEBA: bool>(
  chars: &[char],
  choices: &mut [u16],
  boundaries: &[u8],
) {
  // Negative log10 probabilities avoid underflow regardless of input length.
  // Built-in phrases: 2e-8; unmatched characters: 1e-13. On a tie choose the
  // longer phrase at the current position, making overlap handling deterministic.
  const WORD_COST: f64 = 7.698970004336019;
  const UNKNOWN_COST: f64 = 13.0;
  let mut costs = [0.0; MAX_PHRASE_LEN + 1];
  for i in (0..chars.len()).rev() {
    let mut best = UNKNOWN_COST + costs[(i + 1) % costs.len()];
    let mut state = root(chars[i]);
    let mut crossing_cost = 0.0;
    if state != 0 {
      for (end, ch) in chars
        .iter()
        .enumerate()
        .take((i + MAX_PHRASE_LEN).min(chars.len()))
        .skip(i + 1)
      {
        let Some(next) = child(state, *ch) else { break };
        state = next;
        if JIEBA {
          // One negative-log10 unit per crossed word boundary. This is a
          // preference between competing phrases, not a ban on crossing words.
          crossing_cost += f64::from(boundaries[end]);
        }
        let phrase = NODES[state].phrase;
        if phrase != 0 {
          let score = WORD_COST + crossing_cost + costs[(end + 1) % costs.len()];
          if score <= best {
            best = score;
            choices[i] = phrase;
          }
        }
      }
    }
    costs[i % costs.len()] = best;
  }
  // Rewrite phrase choices in place as the selected syllable IDs. Choices at
  // positions covered by a selected phrase are unreachable in the optimal path.
  let mut i = 0;
  while i < choices.len() {
    let phrase = choices[i];
    if phrase == 0 {
      i += 1;
    } else {
      let syllables = PHRASES[usize::from(phrase)];
      choices[i..i + syllables.len()].copy_from_slice(syllables);
      i += syllables.len();
    }
  }
}

/// Return borrowed syllables and unchanged input runs. Character mode allocates
/// only the result vector; phrase mode also allocates disambiguation scratch space.
pub fn convert(input: &str, style: Style, phrases: bool) -> Vec<&str> {
  tokens(input, phrases)
    .map(|token| token.text(input, style))
    .collect()
}

/// Append delimited output into a reusable caller-owned buffer. Existing contents
/// are preserved; the separator is inserted only between tokens from this call.
pub fn write_pinyin(
  input: &str,
  style: Style,
  phrases: bool,
  separator: &str,
  output: &mut String,
) {
  if input.is_ascii() {
    output.push_str(input);
    return;
  }
  let overrides = if phrases {
    phrase_readings(input)
  } else {
    Prepared::default()
  };
  if separator.is_empty() {
    write_characters::<false>(input, style, separator, overrides, output);
  } else {
    write_characters::<true>(input, style, separator, overrides, output);
  }
}

#[inline]
fn write_characters<const SEPARATED: bool>(
  input: &str,
  style: Style,
  separator: &str,
  overrides: Prepared,
  output: &mut String,
) {
  fn run<const SEPARATED: bool>(
    input: &str,
    style: Style,
    separator: &str,
    mut cursor: impl CharacterCursor,
    mut choices: std::vec::IntoIter<u16>,
    output: &mut String,
  ) {
    let table = &STYLES[style as usize];
    let mut first = true;
    let mut unchanged = None;
    while let Some((i, ch)) = cursor.next() {
      let selected = choices.next().unwrap_or(0);
      let id = if selected != 0 {
        selected
      } else {
        entry(ch) as u16
      };
      if id != 0 {
        if let Some(start) = unchanged.take() {
          if SEPARATED && !first {
            output.push_str(separator);
          }
          output.push_str(&input[start..i]);
          first = false;
        }
        if SEPARATED && !first {
          output.push_str(separator);
        }
        output.push_str(table[id as usize]);
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
        output.push_str(separator);
      }
      output.push_str(&input[start..]);
    }
  }
  let choices = overrides.choices.into_iter();
  if overrides.chars.is_empty() {
    run::<SEPARATED>(
      input,
      style,
      separator,
      RawChars {
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
      DecodedChars {
        chars: overrides.chars.into_iter(),
        position: 0,
      },
      choices,
      output,
    );
  }
}

/// Build delimited output with reserved capacity and amortized growth.
pub fn pinyin(input: &str, style: Style, phrases: bool, separator: &str) -> String {
  let mut output = String::with_capacity(input.len().saturating_mul(2));
  write_pinyin(input, style, phrases, separator, &mut output);
  output
}

// Preserve the existing comparator's exact replacement semantics for non-Han
// accented text as well as Han syllables, without 24 whole-string replacements.
fn comparison_bytes(input: &str) -> impl Iterator<Item = u8> + '_ {
  input.chars().flat_map(|ch| {
    let mut bytes = [0; 16];
    let mut len = 0;
    if let Some(syllable) = lookup(ch) {
      let key = SORT_KEYS[syllable.0 as usize].as_bytes();
      bytes[..key.len()].copy_from_slice(key);
      return bytes.into_iter().take(key.len());
    }
    let mut buf = [0; 4];
    let tone = ch.encode_utf8(&mut buf);
    for c in tone.chars() {
      let replacement = match c {
        'ā' => "a1",
        'á' => "a2",
        'ǎ' => "a3",
        'à' => "a4",
        'ē' => "e1",
        'é' => "e2",
        'ě' => "e3",
        'è' => "e4",
        'ī' => "i1",
        'í' => "i2",
        'ǐ' => "i3",
        'ì' => "i4",
        'ō' => "o1",
        'ó' => "o2",
        'ǒ' => "o3",
        'ò' => "o4",
        'ū' => "u1",
        'ú' => "u2",
        'ǔ' => "u3",
        'ù' => "u4",
        'ǖ' => "ü1",
        'ǘ' => "ü2",
        'ǚ' => "ü3",
        'ǜ' => "ü4",
        _ => "",
      };
      let mut raw = [0; 4];
      let part = if replacement.is_empty() {
        c.encode_utf8(&mut raw)
      } else {
        replacement
      };
      bytes[len..len + part.len()].copy_from_slice(part.as_bytes());
      len += part.len();
    }
    bytes.into_iter().take(len)
  })
}

/// Lexicographic comparison of the legacy tone-number sort key, with early exit.
pub fn compare(a: &str, b: &str) -> Ordering {
  comparison_bytes(a).cmp(comparison_bytes(b))
}
