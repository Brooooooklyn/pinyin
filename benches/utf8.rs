//! Isolate input validation from pinyin conversion. Outputs JSON Lines on stdout.
use std::hint::black_box;
use std::time::{Duration, Instant};

type Outcome = Result<usize, (usize, Option<usize>)>;
type Validator = fn(&[u8]) -> Outcome;
type StringValidator = fn(&[u8]) -> Result<&str, (usize, Option<usize>)>;

fn standard_string(bytes: &[u8]) -> Result<&str, (usize, Option<usize>)> {
  std::str::from_utf8(bytes).map_err(|e| (e.valid_up_to(), e.error_len()))
}

fn compat_string(bytes: &[u8]) -> Result<&str, (usize, Option<usize>)> {
  simdutf8::compat::from_utf8(bytes).map_err(|e| (e.valid_up_to(), e.error_len()))
}

// One shared conversion call site in one executable: only the validator pointer changes.
#[inline(never)]
fn convert(
  bytes: &[u8],
  validate: StringValidator,
  jieba: Option<&pinyin_core::jieba::Jieba>,
) -> String {
  let text = validate(bytes).unwrap();
  match jieba {
    Some(jieba) => pinyin_core::jieba::pinyin(text, pinyin_core::Style::Tone, jieba, false, " "),
    None => pinyin_core::pinyin(text, pinyin_core::Style::Tone, false, " "),
  }
}

fn conversion_benchmark() {
  let corpus = std::fs::read_to_string("benchmark/long.txt").unwrap();
  let literature: String = corpus.chars().cycle().take(100_000).collect();
  let mixed = "中国 API é🙂 \0音乐，2026! ".repeat(5000);
  let jieba = pinyin_core::jieba::Jieba::new();
  let validators: [(&str, StringValidator); 2] =
    [("std", standard_string), ("simd-compat", compat_string)];
  for (name, text) in [("literature-100k", literature), ("mixed-100k", mixed)] {
    let bytes = text.as_bytes();
    for (resolver, segmenter) in [("character", None), ("jieba", Some(&jieba))] {
      assert_eq!(
        convert(bytes, standard_string, segmenter),
        convert(bytes, compat_string, segmenter)
      );
      for (_, validate) in validators {
        for _ in 0..32 {
          black_box(convert(black_box(bytes), black_box(validate), segmenter));
        }
      }
      for round in 0..7 {
        for j in 0..validators.len() {
          let (implementation, validate) = validators[(round + j) % validators.len()];
          let start = Instant::now();
          let mut iterations = 0u64;
          let elapsed = loop {
            black_box(convert(black_box(bytes), black_box(validate), segmenter));
            iterations += 1;
            let elapsed = start.elapsed();
            if elapsed >= Duration::from_millis(150) {
              break elapsed;
            }
          };
          println!(
            "{{\"fixture\":\"{name}\",\"resolver\":\"{resolver}\",\"bytes\":{},\"implementation\":\"{implementation}\",\"round\":{round},\"iterations\":{iterations},\"elapsedNs\":{},\"ns\":{}}}",
            bytes.len(), elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64
          );
        }
      }
    }
  }
}

fn standard(bytes: &[u8]) -> Outcome {
  std::str::from_utf8(bytes)
    .map(str::len)
    .map_err(|e| (e.valid_up_to(), e.error_len()))
}

fn compat(bytes: &[u8]) -> Outcome {
  simdutf8::compat::from_utf8(bytes)
    .map(str::len)
    .map_err(|e| (e.valid_up_to(), e.error_len()))
}

fn basic_with_fallback(bytes: &[u8]) -> Outcome {
  match simdutf8::basic::from_utf8(bytes) {
    Ok(value) => Ok(value.len()),
    Err(_) => standard(bytes),
  }
}

fn verify(bytes: &[u8]) {
  assert_eq!(standard(bytes), compat(bytes));
  assert_eq!(standard(bytes), basic_with_fallback(bytes));
  if let Err(expected) = std::str::from_utf8(bytes) {
    assert_eq!(
      expected.to_string(),
      simdutf8::compat::from_utf8(bytes).unwrap_err().to_string()
    );
  }
}

fn check_boundaries() {
  let valid = "aé中国🙂\0".repeat(600).into_bytes();
  for offset in 0..64 {
    for end in offset..valid.len().min(offset + 256) {
      verify(&valid[offset..end]);
    }
  }
  for offset in [0, 1, 63, 64, 65, 127, 128, 129, 4095] {
    for replacement in 0..=255 {
      let mut bytes = valid.clone();
      bytes[offset] = replacement;
      verify(&bytes);
    }
  }
  let mut seed = 123456789u32;
  for len in 0..1024 {
    let mut bytes = Vec::with_capacity(len);
    for _ in 0..len {
      seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
      bytes.push((seed >> 24) as u8);
    }
    verify(&bytes);
  }
}

fn main() {
  if std::env::var_os("PINYIN_BENCH_CONVERSION").is_some() {
    conversion_benchmark();
    return;
  }
  check_boundaries();
  eprintln!("UTF-8 oracle checks passed: slices, mutations, random bytes, exact diagnostics");
  let mut fixtures = Vec::new();
  for size in [12, 64, 1024, 300_000, 3_000_000] {
    for (name, unit) in [
      ("ascii", "abc123"),
      ("chinese", "重庆银行"),
      ("mixed", "aé中🙂"),
    ] {
      let mut value = unit.repeat(size / unit.len() + 1);
      let mut end = size;
      while !value.is_char_boundary(end) {
        end -= 1;
      }
      value.truncate(end);
      fixtures.push((format!("{name}-{size}"), value.into_bytes()));
    }
  }
  for (name, offset, incomplete) in [
    ("invalid-start-3m", 0, false),
    ("invalid-middle-3m", 1_500_000, false),
    ("invalid-end-3m", 2_999_999, false),
    ("truncated-end-3m", 2_999_999, true),
  ] {
    let mut bytes = "中文".repeat(500_000).into_bytes();
    if incomplete {
      bytes.truncate(offset);
    } else {
      bytes[offset] = 0xff;
    }
    fixtures.push((name.to_owned(), bytes));
  }
  let validators: [(&str, Validator); 3] = [
    ("std", standard),
    ("simd-compat", compat),
    ("simd-basic-std-fallback", basic_with_fallback),
  ];
  let duration = Duration::from_millis(100);
  for (name, bytes) in fixtures {
    verify(&bytes);
    for (_, validate) in validators {
      for _ in 0..64 {
        let _ = black_box(validate(black_box(&bytes)));
      }
    }
    for round in 0..7 {
      for j in 0..validators.len() {
        let (implementation, validate) = validators[(round + j) % validators.len()];
        let start = Instant::now();
        let mut iterations = 0u64;
        let elapsed = loop {
          for _ in 0..64 {
            let _ = black_box(validate(black_box(&bytes)));
          }
          iterations += 64;
          let elapsed = start.elapsed();
          if elapsed >= duration {
            break elapsed;
          }
        };
        println!(
          "{{\"fixture\":\"{name}\",\"bytes\":{},\"implementation\":\"{implementation}\",\"round\":{round},\"iterations\":{iterations},\"elapsedNs\":{},\"ns\":{}}}",
          bytes.len(), elapsed.as_nanos(), elapsed.as_nanos() as f64 / iterations as f64
        );
      }
    }
  }
}
