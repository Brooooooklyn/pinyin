//! Research-only kernels. Does not change the production library.
use pinyin_core::{jieba::Jieba, Style, Token, Tokens};
use serde_json::json;
use std::{
  hint::black_box,
  time::{Duration, Instant},
};

fn utf16_std(s: &str) -> Vec<u16> {
  s.encode_utf16().collect()
}
fn utf16_reserved(s: &str) -> Vec<u16> {
  let mut out = Vec::with_capacity(s.len());
  out.extend(s.encode_utf16());
  out
}
fn utf16_encoding_rs(s: &str) -> Vec<u16> {
  let mut out = vec![0; s.len()];
  let len = encoding_rs::mem::convert_str_to_utf16(s, &mut out);
  out.truncate(len);
  out
}
fn utf16_simd(s: &str) -> Vec<u16> {
  let mut out: Vec<u16> = Vec::with_capacity(s.len());
  // SAFETY: &str is valid UTF-8; input/output are disjoint and aligned;
  // UTF-16 needs at most one u16 per input byte. Publish only initialized units.
  unsafe {
    let len = simdutf::convert_valid_utf8_to_utf16(s.as_ptr(), s.len(), out.as_mut_ptr());
    assert!(len <= out.capacity());
    out.set_len(len);
  }
  out
}
fn chars_simd(s: &str) -> Vec<char> {
  let count = simdutf::count_utf8(s.as_bytes());
  let mut out: Vec<char> = Vec::with_capacity(count);
  // SAFETY: char has u32 layout; valid UTF-8 converts only to Unicode scalars.
  // count_utf8 gives the exact output count. The new allocation cannot overlap input.
  unsafe {
    let len = simdutf::convert_valid_utf8_to_utf32(s.as_ptr(), s.len(), out.as_mut_ptr().cast());
    assert_eq!(len, count);
    out.set_len(len);
  }
  out
}

fn escape_scalar(text: &str, out: &mut String) {
  for ch in text.chars() {
    match ch {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      '\0'..='\u{1f}' => {
        use std::fmt::Write;
        write!(out, "\\u{:04x}", ch as u32).unwrap();
      }
      _ => out.push(ch),
    }
  }
}
fn escapable(b: u8) -> bool {
  b < 32 || b == b'"' || b == b'\\'
}
fn find_escape(bytes: &[u8]) -> Option<usize> {
  let mut i = 0;
  #[cfg(target_arch = "aarch64")]
  {
    use std::arch::aarch64::*;
    // SAFETY: NEON is baseline on this AArch64 target. Every load lies fully
    // inside bytes; no alignment requirement for vld1q_u8. Tail is scalar.
    unsafe {
      while i + 16 <= bytes.len() {
        let v = vld1q_u8(bytes.as_ptr().add(i));
        let mask = vorrq_u8(
          vcltq_u8(v, vdupq_n_u8(32)),
          vorrq_u8(
            vceqq_u8(v, vdupq_n_u8(b'"')),
            vceqq_u8(v, vdupq_n_u8(b'\\')),
          ),
        );
        if vmaxvq_u8(mask) != 0 {
          break;
        }
        i += 16;
      }
    }
  }
  bytes[i..]
    .iter()
    .position(|b| escapable(*b))
    .map(|offset| offset + i)
}
fn escape_simd(text: &str, out: &mut String) {
  let mut start = 0;
  while let Some(offset) = find_escape(&text.as_bytes()[start..]) {
    let index = start + offset;
    // All escaped bytes are ASCII, therefore each cut is a UTF-8 boundary.
    out.push_str(&text[start..index]);
    escape_scalar(&text[index..index + 1], out);
    start = index + 1;
  }
  out.push_str(&text[start..]);
}
type Escape = fn(&str, &mut String);
fn json_output(
  input: &str,
  tokens: impl Iterator<Item = Token>,
  style: Style,
  escape: Escape,
) -> String {
  let mut joined = String::with_capacity(input.len().saturating_mul(2));
  joined.push('[');
  for (i, token) in tokens.enumerate() {
    if i != 0 {
      joined.push(',');
    }
    joined.push('"');
    if token.syllable().is_some() {
      joined.push_str(token.text(input, style));
    } else {
      escape(token.text(input, style), &mut joined);
    }
    joined.push('"');
  }
  joined.push(']');
  joined
}
fn tokens<'a>(input: &'a str, resolver: &str, jieba: &Jieba) -> Tokens<'a> {
  match resolver {
    "jieba" => pinyin_core::jieba::tokens(input, jieba, false),
    "phrase" => pinyin_core::tokens(input, true),
    _ => pinyin_core::tokens(input, false),
  }
}
fn joined(input: &str, style: Style, resolver: &str, jieba: &Jieba) -> String {
  match resolver {
    "jieba" => pinyin_core::jieba::pinyin(input, style, jieba, false, " "),
    "phrase" => pinyin_core::pinyin(input, style, true, " "),
    _ => pinyin_core::pinyin(input, style, false, " "),
  }
}
fn direct_utf16(input: &str, tokens: impl Iterator<Item = Token>, cache: &[Vec<u16>]) -> Vec<u16> {
  let mut out = Vec::with_capacity(input.len() * 2);
  for (i, token) in tokens.enumerate() {
    if i != 0 {
      out.push(32);
    }
    if let Some(s) = token.syllable() {
      out.extend_from_slice(&cache[s.cache_index()]);
    } else {
      out.extend(token.text(input, Style::Tone).encode_utf16());
    }
  }
  out
}

type Case<'a> = (&'a str, Box<dyn Fn() -> usize + 'a>);

fn measure(fixture: &str, operation: &str, input_bytes: usize, calls: Vec<Case<'_>>) {
  for (_, call) in &calls {
    for _ in 0..32 {
      black_box(call());
    }
  }
  for round in 0..7 {
    for j in 0..calls.len() {
      let (implementation, call) = &calls[(round + j) % calls.len()];
      let start = Instant::now();
      let mut iterations = 0;
      let elapsed = loop {
        for _ in 0..8 {
          black_box(call());
        }
        iterations += 8;
        let elapsed = start.elapsed();
        if elapsed >= Duration::from_millis(100) {
          break elapsed;
        }
      };
      println!(
        "{}",
        json!({"fixture":fixture,"operation":operation,"inputBytes":input_bytes,
                "implementation":implementation,"round":round,"iterations":iterations,
                "elapsedNs":elapsed.as_nanos() as u64,"ns":elapsed.as_nanos() as f64 / iterations as f64})
      );
    }
  }
}

fn verify() {
  let text = "aé中🙂\0\"\\\n\u{1f}\u{2028}".repeat(100);
  for start in 0..128 {
    if !text.is_char_boundary(start) {
      continue;
    }
    for end in start..(start + 256).min(text.len()) {
      if !text.is_char_boundary(end) {
        continue;
      }
      let s = &text[start..end];
      assert_eq!(utf16_std(s), utf16_simd(s));
      assert_eq!(utf16_std(s), utf16_encoding_rs(s));
      assert_eq!(s.chars().collect::<Vec<_>>(), chars_simd(s));
      let (mut a, mut b) = (String::new(), String::new());
      escape_scalar(s, &mut a);
      escape_simd(s, &mut b);
      assert_eq!(a, b);
    }
  }
  let all_scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
  assert_eq!(utf16_std(&all_scalars), utf16_simd(&all_scalars));
  assert_eq!(
    all_scalars.chars().collect::<Vec<_>>(),
    chars_simd(&all_scalars)
  );
  for offset in 0..64 {
    for byte in 0..128u8 {
      let s = "x".repeat(offset) + &char::from(byte).to_string() + &"x".repeat(128);
      let (mut a, mut b) = (String::new(), String::new());
      escape_scalar(&s, &mut a);
      escape_simd(&s, &mut b);
      assert_eq!(a, b);
    }
  }
  eprintln!("Passed all-scalar transcoding, unaligned slices, tails, and escaping oracle checks");
}
fn main() {
  verify();
  if std::env::var_os("VERIFY_ONLY").is_some() {
    return;
  }
  let corpus = std::fs::read_to_string("benchmark/long.txt").unwrap();
  let fixtures = [
    ("short", "重庆银行音乐你好".to_owned()),
    ("literature-1k", corpus.chars().take(1000).collect()),
    (
      "literature-100k",
      corpus.chars().cycle().take(100000).collect(),
    ),
    ("mixed-100k", "中国 API é🙂 \0音乐，2026! ".repeat(5000)),
    (
      "ascii-run",
      "Chinese API text_without_escapes/123; ".repeat(10000),
    ),
  ];
  let jieba = Jieba::new();
  let mut cache = vec![Vec::new(); 65536];
  for ch in (0..=0x10ffff).filter_map(char::from_u32) {
    if let Some(s) = pinyin_core::lookup(ch) {
      if cache[s.cache_index()].is_empty() {
        cache[s.cache_index()] = utf16_std(s.text(Style::Tone));
      }
    }
  }
  for (name, input) in &fixtures {
    let bytes = input.len();
    measure(
      name,
      "decode-input-scalars",
      bytes,
      vec![
        (
          "std-chars",
          Box::new(|| black_box(input.chars().collect::<Vec<_>>()).len()),
        ),
        (
          "simdutf-count-and-decode",
          Box::new(|| black_box(chars_simd(black_box(input))).len()),
        ),
      ],
    );
    for (format, output) in [
      ("joined", joined(input, Style::Tone, "character", &jieba)),
      (
        "json",
        json_output(
          input,
          tokens(input, "character", &jieba),
          Style::Tone,
          escape_scalar,
        ),
      ),
    ] {
      assert_eq!(utf16_std(&output), utf16_simd(&output));
      assert_eq!(utf16_std(&output), utf16_encoding_rs(&output));
      measure(
        name,
        &format!("utf16-{format}"),
        output.len(),
        vec![
          (
            "std-collect",
            Box::new(|| black_box(utf16_std(black_box(&output))).len()),
          ),
          (
            "std-reserved",
            Box::new(|| black_box(utf16_reserved(black_box(&output))).len()),
          ),
          (
            "encoding-rs-stable",
            Box::new(|| black_box(utf16_encoding_rs(black_box(&output))).len()),
          ),
          (
            "simdutf",
            Box::new(|| black_box(utf16_simd(black_box(&output))).len()),
          ),
        ],
      );
    }
    if *name == "ascii-run" {
      measure(
        name,
        "escape-single-run",
        bytes,
        vec![
          (
            "scalar",
            Box::new(|| {
              let mut out = String::with_capacity(bytes);
              escape_scalar(input, &mut out);
              black_box(out).len()
            }),
          ),
          (
            "neon-scan-copy",
            Box::new(|| {
              let mut out = String::with_capacity(bytes);
              escape_simd(input, &mut out);
              black_box(out).len()
            }),
          ),
        ],
      );
      continue;
    }
    for resolver in ["character", "phrase", "jieba"] {
      let expected = joined(input, Style::Tone, resolver, &jieba);
      assert_eq!(
        utf16_std(&expected),
        direct_utf16(input, tokens(input, resolver, &jieba), &cache)
      );
      let a = json_output(
        input,
        tokens(input, resolver, &jieba),
        Style::Tone,
        escape_scalar,
      );
      let b = json_output(
        input,
        tokens(input, resolver, &jieba),
        Style::Tone,
        escape_simd,
      );
      assert_eq!(a, b);
      assert_eq!(
        serde_json::from_str::<Vec<String>>(&a).unwrap(),
        tokens(input, resolver, &jieba)
          .map(|t| t.text(input, Style::Tone).to_owned())
          .collect::<Vec<_>>()
      );
      measure(
        name,
        &format!("joined-total-{resolver}"),
        bytes,
        vec![
          (
            "current-scalar-utf16",
            Box::new(|| {
              black_box(utf16_std(&joined(
                black_box(input),
                Style::Tone,
                resolver,
                &jieba,
              )))
              .len()
            }),
          ),
          (
            "simd-utf16",
            Box::new(|| {
              black_box(utf16_simd(&joined(
                black_box(input),
                Style::Tone,
                resolver,
                &jieba,
              )))
              .len()
            }),
          ),
          (
            "direct-cached-utf16",
            Box::new(|| {
              black_box(direct_utf16(
                black_box(input),
                tokens(input, resolver, &jieba),
                &cache,
              ))
              .len()
            }),
          ),
        ],
      );
      measure(
        name,
        &format!("json-total-{resolver}"),
        bytes,
        vec![
          (
            "current-scalar-utf16",
            Box::new(|| {
              black_box(utf16_std(&json_output(
                input,
                tokens(input, resolver, &jieba),
                Style::Tone,
                escape_scalar,
              )))
              .len()
            }),
          ),
          (
            "simd-utf16",
            Box::new(|| {
              black_box(utf16_simd(&json_output(
                input,
                tokens(input, resolver, &jieba),
                Style::Tone,
                escape_scalar,
              )))
              .len()
            }),
          ),
          (
            "simd-utf16-and-escape",
            Box::new(|| {
              black_box(utf16_simd(&json_output(
                input,
                tokens(input, resolver, &jieba),
                Style::Tone,
                escape_simd,
              )))
              .len()
            }),
          ),
        ],
      );
    }
    measure(
      name,
      "jieba-cut-only",
      bytes,
      vec![(
        "jieba-hmm-false",
        Box::new(|| black_box(jieba.cut(black_box(input), false)).len()),
      )],
    );
  }
}
