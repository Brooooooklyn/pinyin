use pinyin::{Pinyin, ToPinyin};
use pinyin_core::Style;
use std::{
  hint::black_box,
  time::{Duration, Instant},
};

fn measure(name: &str, characters: usize, mut run: impl FnMut()) {
  let mut samples = Vec::new();
  for _ in 0..7 {
    let start = Instant::now();
    let mut iterations = 0u64;
    while start.elapsed() < Duration::from_millis(100) {
      for _ in 0..32 {
        run();
        iterations += 1;
      }
    }
    samples.push(start.elapsed().as_nanos() as f64 / iterations as f64);
  }
  samples.sort_by(f64::total_cmp);
  let median = samples[3];
  println!(
    "{name}: median {median:.1} ns/op, {:.2} ns/char; samples {samples:?}",
    median / characters as f64
  );
}

fn main() {
  let path = std::env::var_os("PINYIN_BENCH_TEXT")
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|| {
      std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmark/long.txt")
    });
  let text = std::fs::read_to_string(&path).expect("set PINYIN_BENCH_TEXT to a UTF-8 corpus file");
  println!("corpus: {}; UTF-8 bytes: {}", path.display(), text.len());
  for input in ["你好拼音", text.as_str()] {
    let characters = input.chars().count();
    let mut output = String::with_capacity(input.len() * 3);
    let expected = pinyin_core::pinyin(input, Style::Tone, false, "");
    let legacy: String = input
      .chars()
      .map(|c| {
        c.to_pinyin()
          .map(Pinyin::with_tone)
          .map(str::to_owned)
          .unwrap_or_else(|| c.to_string())
      })
      .collect();
    assert_eq!(expected, legacy);
    measure("legacy lookup + reusable output", characters, || {
      output.clear();
      for ch in black_box(input).chars() {
        if let Some(py) = ch.to_pinyin() {
          output.push_str(py.with_tone());
        } else {
          output.push(ch);
        }
      }
      black_box(&output);
    });
    measure("core lookup + reusable output", characters, || {
      output.clear();
      pinyin_core::write_pinyin(black_box(input), Style::Tone, false, "", &mut output);
      black_box(&output);
    });
    measure("core phrase + reusable output", characters, || {
      output.clear();
      pinyin_core::write_pinyin(black_box(input), Style::Tone, true, "", &mut output);
      black_box(&output);
    });
    measure("core + owned output", characters, || {
      black_box(pinyin_core::pinyin(
        black_box(input),
        Style::Tone,
        false,
        "",
      ));
    });
  }
}
