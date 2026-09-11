#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use crate::build_error::{index, require, BuildError, Result};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Default)]
struct Node {
  children: BTreeMap<char, usize>,
  phrase: u16,
}

fn styles(tone: &str) -> Result<[String; 5]> {
  // The binding can emit dictionary syllables directly inside JSON strings.
  // Enforce that invariant when data changes, rather than escaping every lookup.
  require(
    !tone
      .chars()
      .any(|ch| ch <= '\u{1f}' || ch == '"' || ch == '\\'),
    format!("syllable {tone:?} contains a character requiring JSON escaping"),
  )?;
  let mut plain = String::new();
  let mut number = String::new();
  let mut last_tone = None;
  for ch in tone.chars() {
    let (letter, digit) = match ch {
      'ā' => (Some('a'), Some('1')),
      'á' => (Some('a'), Some('2')),
      'ǎ' => (Some('a'), Some('3')),
      'à' => (Some('a'), Some('4')),
      'ē' => (Some('e'), Some('1')),
      'é' => (Some('e'), Some('2')),
      'ě' => (Some('e'), Some('3')),
      'è' => (Some('e'), Some('4')),
      'ế' => (Some('ê'), Some('2')),
      'ề' => (Some('ê'), Some('4')),
      'ī' => (Some('i'), Some('1')),
      'í' => (Some('i'), Some('2')),
      'ǐ' => (Some('i'), Some('3')),
      'ì' => (Some('i'), Some('4')),
      'ō' => (Some('o'), Some('1')),
      'ó' => (Some('o'), Some('2')),
      'ǒ' => (Some('o'), Some('3')),
      'ò' => (Some('o'), Some('4')),
      'ū' => (Some('u'), Some('1')),
      'ú' => (Some('u'), Some('2')),
      'ǔ' => (Some('u'), Some('3')),
      'ù' => (Some('u'), Some('4')),
      'ǖ' => (Some('ü'), Some('1')),
      'ǘ' => (Some('ü'), Some('2')),
      'ǚ' => (Some('ü'), Some('3')),
      'ǜ' => (Some('ü'), Some('4')),
      'ń' => (Some('n'), Some('2')),
      'ň' => (Some('n'), Some('3')),
      'ǹ' => (Some('n'), Some('4')),
      'ḿ' => (Some('m'), Some('2')),
      '\u{304}' => (None, Some('1')),
      '\u{30c}' => (None, Some('3')),
      '\u{300}' => (None, Some('4')),
      _ => (Some(ch), None),
    };
    if let Some(letter) = letter {
      plain.push(letter);
      number.push(letter);
    }
    if let Some(digit) = digit {
      require(
        last_tone.replace(digit).is_none(),
        format!("multiple tones in syllable {tone:?}"),
      )?;
      number.push(digit);
    }
  }
  let mut end = plain.clone();
  if let Some(digit) = last_tone {
    end.push(digit);
  }
  let first = plain.chars().next().map(String::from).unwrap_or_default();
  Ok([plain, tone.to_owned(), number, end, first])
}

fn syllable(
  value: &str,
  ids: &mut BTreeMap<String, u16>,
  values: &mut Vec<[String; 5]>,
) -> Result<u16> {
  if let Some(id) = ids.get(value) {
    return Ok(*id);
  }
  let id = index(values.len(), "syllable")?;
  ids.insert(value.to_owned(), id);
  values.push(styles(value)?);
  Ok(id)
}

// Share identical 256-code-point pages, including the all-zero page. Native Rust
// integers keep the generated dictionary portable to big-endian/32-bit targets.
fn pages(name: &str, values: &[u32], output: &mut String) -> Result<()> {
  let mut unique = BTreeMap::new();
  let mut data = vec![0; 256];
  unique.insert(vec![0u32; 256], 0u16);
  let mut index = Vec::new();
  for page in values.chunks(256) {
    let id = if let Some(&id) = unique.get(page) {
      id
    } else {
      let id = crate::build_error::index(data.len() / 256, "dictionary page")?;
      data.extend_from_slice(page);
      unique.insert(page.to_vec(), id);
      id
    };
    index.push(id);
  }
  writeln!(output, "static {name}_PAGES: &[u16] = &{index:?};")?;
  writeln!(output, "static {name}_DATA: &[u32] = &{data:?};")?;
  Ok(())
}

pub fn generate(characters: &str, phrase_data: &str, utf16: bool, simd: bool) -> Result<String> {
  let mut ids = BTreeMap::new();
  let mut values = Vec::new();
  syllable("", &mut ids, &mut values)?;
  let mut entries = vec![0u32; 0x110000];
  let mut reading_ids = BTreeMap::<Vec<u16>, u16>::new();
  let mut readings = vec![vec![]];
  reading_ids.insert(vec![], 0);
  for (line, raw) in characters.lines().enumerate() {
    let context = |message: &str| {
      BuildError::InvalidData(format!("data/characters.txt:{}: {message}", line + 1))
    };
    let raw = raw.split_once('#').map_or(raw, |(entry, _)| entry).trim();
    if raw.is_empty() {
      continue;
    }
    let (code, text) = raw
      .split_once(':')
      .ok_or_else(|| context("expected U+<hex>: <readings>"))?;
    let code = code
      .trim()
      .strip_prefix("U+")
      .ok_or_else(|| context("character code must start with U+"))?;
    let code =
      u32::from_str_radix(code, 16).map_err(|_| context("character code must be hexadecimal"))?;
    if char::from_u32(code).is_none() {
      return Err(context("character code is not a Unicode scalar"));
    }
    if entries[code as usize] != 0 {
      return Err(context("duplicate character"));
    }
    if text.trim().is_empty() || text.split(',').any(|s| s.trim().is_empty()) {
      return Err(context("character must have nonempty readings"));
    }
    let list: Vec<_> = text
      .trim()
      .split(',')
      .map(|s| syllable(s, &mut ids, &mut values).map_err(|error| context(&error.to_string())))
      .collect::<Result<_>>()?;
    let multi = if let Some(&id) = reading_ids.get(&list) {
      id
    } else {
      let id = index(readings.len(), "reading list")?;
      readings.push(list.clone());
      reading_ids.insert(list.clone(), id);
      id
    };
    entries[code as usize] = u32::from(list[0]) | (u32::from(multi) << 16);
  }

  require(
    entries[..128].iter().all(|&entry| entry == 0),
    "ASCII run fast path requires unmapped ASCII",
  )?;
  let mut nodes = vec![Node::default()];
  let mut phrases: Vec<Vec<u16>> = vec![vec![]];
  for (line_number, line) in phrase_data.lines().enumerate() {
    if line.starts_with('#') || line.is_empty() {
      continue;
    }
    let context = |message: &str| {
      BuildError::InvalidData(format!("data/phrases.tsv:{}: {message}", line_number + 1))
    };
    let (word, text) = line
      .split_once('\t')
      .ok_or_else(|| context("expected phrase and readings separated by a tab"))?;
    if word.is_empty() {
      return Err(context("phrase must not be empty"));
    }
    let list: Vec<_> = text
      .split_whitespace()
      .map(|s| syllable(s, &mut ids, &mut values).map_err(|error| context(&error.to_string())))
      .collect::<Result<_>>()?;
    if word.chars().count() != list.len() {
      return Err(context(
        "phrase character count does not match reading count",
      ));
    }
    let mut state = 0;
    for ch in word.chars() {
      if entries[ch as usize] == 0 {
        return Err(context(&format!(
          "phrase character {ch:?} has no dictionary entry"
        )));
      }
      state = if let Some(next) = nodes[state].children.get(&ch) {
        *next
      } else {
        let next = nodes.len();
        nodes.push(Node::default());
        nodes[state].children.insert(ch, next);
        next
      };
    }
    if nodes[state].phrase != 0 {
      return Err(context(&format!("duplicate phrase {word:?}")));
    }
    nodes[state].phrase = index(phrases.len(), "phrase")?;
    phrases.push(list);
  }
  let mut output = String::from("// Generated by build.rs from pinned, vendored dictionaries.\n");
  writeln!(
    output,
    "pub const SYLLABLE_COUNT: usize = {};",
    values.len()
  )?;
  writeln!(output, "static STYLES: [[&str; SYLLABLE_COUNT]; 5] = [")?;
  for style in 0..5 {
    let strings: Vec<_> = values.iter().map(|s| s[style].as_str()).collect();
    writeln!(output, "{strings:?},")?;
  }
  writeln!(output, "];")?;
  // Preserve the comparator's legacy mapping, which intentionally differs
  // from ToneNumber for a few uncommon accents. No runtime normalization.
  let replacements = [
    ("ā", "a1"),
    ("á", "a2"),
    ("ǎ", "a3"),
    ("à", "a4"),
    ("ē", "e1"),
    ("é", "e2"),
    ("ě", "e3"),
    ("è", "e4"),
    ("ī", "i1"),
    ("í", "i2"),
    ("ǐ", "i3"),
    ("ì", "i4"),
    ("ō", "o1"),
    ("ó", "o2"),
    ("ǒ", "o3"),
    ("ò", "o4"),
    ("ū", "u1"),
    ("ú", "u2"),
    ("ǔ", "u3"),
    ("ù", "u4"),
    ("ǖ", "ü1"),
    ("ǘ", "ü2"),
    ("ǚ", "ü3"),
    ("ǜ", "ü4"),
  ];
  let sort_keys: Vec<_> = values
    .iter()
    .map(|styles| {
      let mut key = styles[1].clone();
      for (from, to) in replacements {
        key = key.replace(from, to);
      }
      require(
        key.len() <= 16,
        format!("sort key {key:?} exceeds the 16-byte comparator buffer"),
      )?;
      Ok(key)
    })
    .collect::<Result<_>>()?;
  writeln!(output, "static SORT_KEYS: &[&str] = &{sort_keys:?};")?;
  // Optional engine-facing representation: no runtime cache or transcoding.
  if utf16 {
    writeln!(
      output,
      "static STYLES_UTF16: [[&[u16]; SYLLABLE_COUNT]; 5] = ["
    )?;
    for style in 0..5 {
      writeln!(output, "[")?;
      for syllable in &values {
        let units: Vec<_> = syllable[style].encode_utf16().collect();
        writeln!(output, "&{units:?},")?;
      }
      writeln!(output, "],")?;
    }
    writeln!(output, "];")?;
  }
  writeln!(
    output,
    "static COMMON: &[u32] = &{:?};",
    &entries[0x4e00..0xa000]
  )?;
  entries[0x4e00..0xa000].fill(0);
  pages("CHAR", &entries, &mut output)?;
  writeln!(output, "static READINGS: &[&[u16]] = &[")?;
  for list in readings {
    writeln!(output, "&{list:?},")?;
  }
  writeln!(output, "];")?;
  let mut roots = vec![0u32; 0x110000];
  for (ch, next) in &nodes[0].children {
    roots[*ch as usize] = index(*next, "trie node")?;
  }
  pages("ROOT", &roots, &mut output)?;
  let mut edges = Vec::<(u32, u32)>::new();
  let mut small_labels = vec![[0u32; 4]];
  writeln!(output, "static NODES: &[Node] = &[")?;
  for node in nodes {
    let mut start = edges.len();
    if simd {
      require(
        start <= u16::MAX as usize,
        "SIMD trie edge offset exceeds u16 capacity",
      )?;
      if (2..=4).contains(&node.children.len()) {
        let mut labels = [0; 4];
        for (slot, ch) in labels.iter_mut().zip(node.children.keys()) {
          *slot = *ch as u32;
        }
        require(
          small_labels.len() <= u16::MAX as usize,
          "SIMD label index exceeds u16 capacity",
        )?;
        start |= small_labels.len() << 16;
        small_labels.push(labels);
      }
    }
    let len = node.children.len();
    require(
      len <= u16::MAX as usize,
      "trie node child count exceeds u16 capacity",
    )?;
    for (ch, next) in &node.children {
      edges.push((*ch as u32, index(*next, "trie node")?));
    }
    writeln!(
      output,
      "Node {{ start: {start}, len: {len}, phrase: {} }},",
      node.phrase
    )?;
  }
  writeln!(output, "];")?;
  if simd {
    writeln!(
      output,
      "static SMALL_LABELS: &[[u32; 4]] = &{small_labels:?};"
    )?;
  }
  writeln!(output, "static EDGES: &[(u32, u32)] = &{edges:?};")?;
  writeln!(
    output,
    "const MAX_PHRASE_LEN: usize = {};",
    phrases
      .iter()
      .map(Vec::len)
      .max()
      .ok_or_else(|| BuildError::InvalidData("missing phrase sentinel".into()))?
  )?;
  writeln!(output, "static PHRASES: &[&[u16]] = &[")?;
  for list in phrases {
    writeln!(output, "&{list:?},")?;
  }
  writeln!(output, "];")?;
  Ok(output)
}
