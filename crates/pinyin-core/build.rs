use std::{collections::BTreeMap, env, fmt::Write, fs, path::PathBuf};

#[derive(Default)]
struct Node {
  children: BTreeMap<char, usize>,
  phrase: u16,
}

fn styles(tone: &str) -> [String; 5] {
  // The binding can emit dictionary syllables directly inside JSON strings.
  // Enforce that invariant when data changes, rather than escaping every lookup.
  assert!(!tone
    .chars()
    .any(|ch| ch <= '\u{1f}' || ch == '"' || ch == '\\'));
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
      assert!(last_tone.replace(digit).is_none(), "multiple tones: {tone}");
      number.push(digit);
    }
  }
  let mut end = plain.clone();
  if let Some(digit) = last_tone {
    end.push(digit);
  }
  let first = plain.chars().next().map(String::from).unwrap_or_default();
  [plain, tone.to_owned(), number, end, first]
}

fn syllable(value: &str, ids: &mut BTreeMap<String, u16>, values: &mut Vec<[String; 5]>) -> u16 {
  if let Some(id) = ids.get(value) {
    return *id;
  }
  let id = values.len().try_into().expect("too many syllables");
  ids.insert(value.to_owned(), id);
  values.push(styles(value));
  id
}

// Share identical 256-code-point pages, including the all-zero page. Native Rust
// integers keep the generated dictionary portable to big-endian/32-bit targets.
fn pages(name: &str, values: &[u32], output: &mut String) {
  let mut unique = BTreeMap::new();
  let mut data = vec![0; 256];
  unique.insert(vec![0u32; 256], 0u16);
  let mut index = Vec::new();
  for page in values.chunks(256) {
    let id = *unique.entry(page.to_vec()).or_insert_with(|| {
      let id = (data.len() / 256).try_into().expect("too many pages");
      data.extend_from_slice(page);
      id
    });
    index.push(id);
  }
  writeln!(output, "static {name}_PAGES: &[u16] = &{index:?};").unwrap();
  writeln!(output, "static {name}_DATA: &[u32] = &{data:?};").unwrap();
}

fn main() {
  println!("cargo:rerun-if-changed=build.rs");
  println!("cargo:rerun-if-changed=data/characters.txt");
  println!("cargo:rerun-if-changed=data/phrases.tsv");
  let mut ids = BTreeMap::new();
  let mut values = Vec::new();
  syllable("", &mut ids, &mut values);
  let mut entries = vec![0u32; 0x110000];
  let mut reading_ids = BTreeMap::<Vec<u16>, u16>::new();
  let mut readings = vec![vec![]];
  reading_ids.insert(vec![], 0);
  for raw in include_str!("data/characters.txt").lines() {
    let raw = raw.split('#').next().unwrap().trim();
    if raw.is_empty() {
      continue;
    }
    let (code, text) = raw.split_once(':').expect("character entry");
    let code = u32::from_str_radix(code.trim().strip_prefix("U+").unwrap(), 16).unwrap();
    assert!(char::from_u32(code).is_some());
    assert_eq!(entries[code as usize], 0, "duplicate character");
    let list: Vec<_> = text
      .trim()
      .split(',')
      .map(|s| syllable(s, &mut ids, &mut values))
      .collect();
    assert!(!list.is_empty());
    let multi = *reading_ids.entry(list.clone()).or_insert_with(|| {
      let id = readings.len().try_into().expect("too many reading lists");
      readings.push(list.clone());
      id
    });
    entries[code as usize] = u32::from(list[0]) | (u32::from(multi) << 16);
  }

  assert!(
    entries[..128].iter().all(|&entry| entry == 0),
    "ASCII run fast path requires unmapped ASCII"
  );
  let mut nodes = vec![Node::default()];
  let mut phrases: Vec<Vec<u16>> = vec![vec![]];
  for line in include_str!("data/phrases.tsv")
    .lines()
    .filter(|l| !l.starts_with('#') && !l.is_empty())
  {
    let (word, text) = line.split_once('\t').expect("phrase entry");
    let list: Vec<_> = text
      .split_whitespace()
      .map(|s| syllable(s, &mut ids, &mut values))
      .collect();
    assert_eq!(word.chars().count(), list.len(), "{word}");
    let mut state = 0;
    for ch in word.chars() {
      assert_ne!(entries[ch as usize], 0, "phrase character missing: {ch}");
      state = if let Some(next) = nodes[state].children.get(&ch) {
        *next
      } else {
        let next = nodes.len();
        nodes.push(Node::default());
        nodes[state].children.insert(ch, next);
        next
      };
    }
    assert_eq!(nodes[state].phrase, 0, "duplicate phrase: {word}");
    nodes[state].phrase = phrases.len().try_into().expect("too many phrases");
    phrases.push(list);
  }
  let mut output = String::from("// Generated by build.rs from pinned, vendored dictionaries.\n");
  writeln!(
    output,
    "pub const SYLLABLE_COUNT: usize = {};",
    values.len()
  )
  .unwrap();
  writeln!(output, "static STYLES: [[&str; SYLLABLE_COUNT]; 5] = [").unwrap();
  for style in 0..5 {
    let strings: Vec<_> = values.iter().map(|s| s[style].as_str()).collect();
    writeln!(output, "{strings:?},").unwrap();
  }
  writeln!(output, "];").unwrap();
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
      assert!(key.len() <= 16);
      key
    })
    .collect();
  writeln!(output, "static SORT_KEYS: &[&str] = &{sort_keys:?};").unwrap();
  // Optional engine-facing representation: no runtime cache or transcoding.
  if env::var_os("CARGO_FEATURE_UTF16").is_some() {
    writeln!(
      output,
      "static STYLES_UTF16: [[&[u16]; SYLLABLE_COUNT]; 5] = ["
    )
    .unwrap();
    for style in 0..5 {
      writeln!(output, "[").unwrap();
      for syllable in &values {
        let units: Vec<_> = syllable[style].encode_utf16().collect();
        writeln!(output, "&{units:?},").unwrap();
      }
      writeln!(output, "],").unwrap();
    }
    writeln!(output, "];").unwrap();
  }
  writeln!(
    output,
    "static COMMON: &[u32] = &{:?};",
    &entries[0x4e00..0xa000]
  )
  .unwrap();
  entries[0x4e00..0xa000].fill(0);
  pages("CHAR", &entries, &mut output);
  writeln!(output, "static READINGS: &[&[u16]] = &[").unwrap();
  for list in readings {
    writeln!(output, "&{list:?},").unwrap();
  }
  writeln!(output, "];").unwrap();
  let mut roots = vec![0u32; 0x110000];
  for (ch, next) in &nodes[0].children {
    roots[*ch as usize] = (*next).try_into().unwrap();
  }
  pages("ROOT", &roots, &mut output);
  let mut edges = Vec::<(u32, u32)>::new();
  let mut small_labels = vec![[0u32; 4]];
  writeln!(output, "static NODES: &[Node] = &[").unwrap();
  for node in nodes {
    let mut start = edges.len();
    if env::var_os("CARGO_FEATURE_SIMD").is_some() {
      assert!(start <= u16::MAX as usize);
      if (2..=4).contains(&node.children.len()) {
        let mut labels = [0; 4];
        for (slot, ch) in labels.iter_mut().zip(node.children.keys()) {
          *slot = *ch as u32;
        }
        assert!(small_labels.len() <= u16::MAX as usize);
        start |= small_labels.len() << 16;
        small_labels.push(labels);
      }
    }
    let len = node.children.len();
    assert!(len <= u16::MAX as usize);
    edges.extend(
      node
        .children
        .iter()
        .map(|(ch, next)| (*ch as u32, (*next).try_into().unwrap())),
    );
    writeln!(
      output,
      "Node {{ start: {start}, len: {len}, phrase: {} }},",
      node.phrase
    )
    .unwrap();
  }
  writeln!(output, "];").unwrap();
  if env::var_os("CARGO_FEATURE_SIMD").is_some() {
    writeln!(
      output,
      "static SMALL_LABELS: &[[u32; 4]] = &{small_labels:?};"
    )
    .unwrap();
  }
  writeln!(output, "static EDGES: &[(u32, u32)] = &{edges:?};").unwrap();
  writeln!(
    output,
    "const MAX_PHRASE_LEN: usize = {};",
    phrases.iter().map(Vec::len).max().unwrap()
  )
  .unwrap();
  writeln!(output, "static PHRASES: &[&[u16]] = &[").unwrap();
  for list in phrases {
    writeln!(output, "&{list:?},").unwrap();
  }
  writeln!(output, "];").unwrap();
  fs::write(
    PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("dictionary.rs"),
    output,
  )
  .unwrap();
}
