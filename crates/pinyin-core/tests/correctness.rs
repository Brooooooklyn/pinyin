use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};
use pinyin_core::{compare, convert, lookup, pinyin, readings, tokens, write_pinyin, Style};

#[test]
fn long_spans_keep_byte_ranges_choices_and_separator_boundaries() {
  for length in [0, 1, 15, 16, 31, 32, 63, 64, 127, 128, 1024] {
    for span in ["a", "\0", "é🙂", "\"\\\r\n", "\u{7f}", "é\u{301}"] {
      let input = format!(
        "{}{}{}",
        "音乐重庆".repeat(64),
        span.repeat(length),
        "银行长大".repeat(64)
      );
      for phrases in [false, true] {
        let tokens: Vec<_> = tokens(&input, phrases).unwrap().collect();
        assert_eq!(
          tokens.iter().map(|t| &input[t.range()]).collect::<String>(),
          input
        );
        assert!(!tokens
          .windows(2)
          .any(|w| w[0].syllable().is_none() && w[1].syllable().is_none()));
        for style in [
          Style::Plain,
          Style::Tone,
          Style::ToneNumber,
          Style::ToneNumberEnd,
          Style::FirstLetter,
        ] {
          for separator in ["", " ", "🙂\0"] {
            let expected = tokens
              .iter()
              .map(|t| t.text(&input, style))
              .collect::<Vec<_>>()
              .join(separator);
            assert_eq!(pinyin(&input, style, phrases, separator).unwrap(), expected);
            #[cfg(feature = "utf16")]
            assert_eq!(
              pinyin_core::utf16::pinyin(&input, style, phrases, separator).unwrap(),
              expected.encode_utf16().collect::<Vec<_>>()
            );
          }
        }
      }
    }
  }
}

type RenderLegacy = fn(Pinyin) -> &'static str;
const STYLES: [(Style, RenderLegacy); 5] = [
  (Style::Plain, Pinyin::plain),
  (Style::Tone, Pinyin::with_tone),
  (Style::ToneNumber, Pinyin::with_tone_num),
  (Style::ToneNumberEnd, Pinyin::with_tone_num_end),
  (Style::FirstLetter, Pinyin::first_letter),
];

#[test]
fn every_unicode_scalar_matches_legacy_in_every_style_and_reading() {
  for ch in (0..=0x10ffff).filter_map(char::from_u32) {
    let old = ch.to_pinyin();
    let new = lookup(ch);
    assert_eq!(old.is_some(), new.is_some(), "U+{:X}", ch as u32);
    for (style, render) in STYLES {
      assert_eq!(
        old.map(render),
        new.map(|s| s.text(style)),
        "{ch:?} {style:?}"
      );
      let expected: Vec<_> = ch
        .to_pinyin_multi()
        .into_iter()
        .flatten()
        .map(render)
        .collect();
      let actual: Vec<_> = readings(ch).map(|s| s.text(style)).collect();
      assert_eq!(expected, actual, "{ch:?} {style:?}");
      #[cfg(feature = "utf16")]
      for syllable in readings(ch) {
        assert_eq!(
          syllable.utf16(style),
          syllable.text(style).encode_utf16().collect::<Vec<_>>()
        );
      }
    }
  }
}

#[test]
fn every_dictionary_phrase_has_its_declared_reading() {
  let mut count = 0;
  for line in include_str!("../data/phrases.tsv")
    .lines()
    .filter(|s| !s.starts_with('#') && !s.is_empty())
  {
    let (word, expected) = line.split_once('\t').unwrap();
    assert_eq!(
      pinyin(word, Style::Tone, true, " ").unwrap(),
      expected,
      "{word}"
    );
    #[cfg(feature = "utf16")]
    assert_eq!(
      pinyin_core::utf16::pinyin(word, Style::Tone, true, " ").unwrap(),
      expected.encode_utf16().collect::<Vec<_>>(),
      "{word}"
    );
    count += 1;
  }
  assert_eq!(count, 4083);
}

#[test]
fn mixed_unicode_and_non_han_runs_are_lossless() {
  for phrase in [false, true] {
    for input in [
      "",
      "ASCII\0\t\n",
      "🙂👨‍👩‍👧‍👦",
      "A\0中\u{301}文 🦀B",
      "𠮷野家𠀀\u{10ffff}",
      "\u{feff}重庆 / 银行！",
    ] {
      let list: Vec<_> = tokens(input, phrase).unwrap().collect();
      let original: String = list.iter().map(|token| &input[token.range()]).collect();
      assert_eq!(original, input);
      for token in list {
        assert!(input.is_char_boundary(token.range().start));
        assert!(input.is_char_boundary(token.range().end));
      }
    }
    assert_eq!(
      convert("abc🙂\0中文 xyz", Style::Plain, phrase).unwrap(),
      ["abc🙂\0", "zhong", "wen", " xyz"]
    );
    assert_eq!(
      convert("", Style::Plain, phrase).unwrap(),
      Vec::<&str>::new()
    );
  }
}

#[test]
fn phrases_resolve_polyphones_without_changing_character_mode() {
  assert_eq!(
    convert("重庆银行音乐", Style::Plain, false).unwrap(),
    ["zhong", "qing", "yin", "xing", "yin", "le"]
  );
  assert_eq!(
    convert("重庆银行音乐", Style::Plain, true).unwrap(),
    ["chong", "qing", "yin", "hang", "yin", "yue"]
  );
  assert_eq!(
    convert("重庆银行音乐", Style::ToneNumberEnd, true).unwrap(),
    ["chong2", "qing4", "yin2", "hang2", "yin1", "yue4"]
  );
}

#[test]
fn phrase_probabilities_do_not_underflow_on_long_inputs() {
  let unit = "重庆银行，音乐快乐！";
  let repeated = unit.repeat(20_000);
  assert_eq!(
    pinyin(&repeated, Style::Tone, true, "").unwrap(),
    pinyin(unit, Style::Tone, true, "").unwrap().repeat(20_000)
  );
}

#[test]
fn rolling_trie_solver_matches_independent_full_dictionary_search() {
  let dict: Vec<_> = include_str!("../data/phrases.tsv")
    .lines()
    .filter(|s| !s.starts_with('#') && !s.is_empty())
    .map(|line| {
      let (word, readings) = line.split_once('\t').unwrap();
      (word.chars().collect::<Vec<_>>(), readings.replace(' ', ""))
    })
    .collect();
  let fragments = [
    "重庆",
    "银行",
    "行长",
    "长大",
    "重新",
    "重重",
    "朝阳",
    "朝阳区",
    "音乐会",
    "会长",
    "快乐",
    "长乐",
    "了解",
    "解放思想",
    "𠮷🙂 ",
    "一行不行",
  ];
  let mut seed = 0x12345678u32;
  for _ in 0..100 {
    let mut input = String::new();
    for _ in 0..10 {
      seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
      input.push_str(fragments[(seed >> 16) as usize % fragments.len()]);
    }
    let chars: Vec<_> = input.chars().collect();
    let mut costs = vec![0.0; chars.len() + 1];
    let mut choices = vec![None; chars.len()];
    for i in (0..chars.len()).rev() {
      costs[i] = 13.0 + costs[i + 1];
      let mut best_len = 1;
      for (index, (word, _)) in dict.iter().enumerate() {
        if chars[i..].starts_with(word) {
          let cost = 7.698970004336019 + costs[i + word.len()];
          if cost < costs[i] || (cost == costs[i] && word.len() > best_len) {
            costs[i] = cost;
            choices[i] = Some(index);
            best_len = word.len();
          }
        }
      }
    }
    let mut expected = String::new();
    let mut i = 0;
    while i < chars.len() {
      if let Some(index) = choices[i] {
        expected.push_str(&dict[index].1);
        i += dict[index].0.len();
      } else {
        if let Some(syllable) = lookup(chars[i]) {
          expected.push_str(syllable.text(Style::Tone));
        } else {
          expected.push(chars[i]);
        }
        i += 1;
      }
    }
    assert_eq!(
      pinyin(&input, Style::Tone, true, "").unwrap(),
      expected,
      "{input}"
    );
  }
}

#[test]
fn reusable_output_appends_and_separators_do_not_change_non_han_text() {
  let mut output = String::from("prefix:");
  write_pinyin("中 文\0🙂", Style::Plain, false, "|", &mut output).unwrap();
  assert_eq!(output, "prefix:zhong| |wen|\0🙂");
  write_pinyin("", Style::Tone, true, "!", &mut output).unwrap();
  assert_eq!(output, "prefix:zhong| |wen|\0🙂");
}

#[cfg(feature = "utf16")]
#[test]
fn utf16_output_matches_token_oracle_and_preserves_reused_buffer() {
  for input in [
    "",
    "ASCII\0\r\n",
    "𠮷👨‍👩‍👧‍👦重庆\u{301} \0🙂银行\r\nZ",
    "A中\"\\\t\u{2028}文",
  ] {
    for (style, _) in STYLES {
      for phrases in [false, true] {
        for separator in ["", " ", "|🙂\0"] {
          let expected = convert(input, style, phrases).unwrap().join(separator);
          let units: Vec<_> = expected.encode_utf16().collect();
          assert_eq!(
            pinyin_core::utf16::pinyin(input, style, phrases, separator).unwrap(),
            units
          );
          let mut output = vec![0xfeed];
          let separator: Vec<_> = separator.encode_utf16().collect();
          pinyin_core::utf16::write_pinyin(input, style, phrases, &separator, &mut output).unwrap();
          assert_eq!(output[0], 0xfeed);
          assert_eq!(&output[1..], units);
        }
      }
    }
  }
}

fn legacy_sort_key(input: &str) -> String {
  let mut result: String = input
    .chars()
    .map(|ch| {
      ch.to_pinyin()
        .map(|py| py.with_tone().to_owned())
        .unwrap_or_else(|| ch.to_string())
    })
    .collect();
  for (ch, replacement) in "āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ".chars().zip([
    "a1", "a2", "a3", "a4", "e1", "e2", "e3", "e4", "i1", "i2", "i3", "i4", "o1", "o2", "o3", "o4",
    "u1", "u2", "u3", "u4", "ü1", "ü2", "ü3", "ü4",
  ]) {
    result = result.replace(ch, replacement);
  }
  result
}

#[test]
fn streaming_comparator_preserves_legacy_byte_order() {
  let inputs = [
    "",
    "蜘蛛侠1",
    "蜘蛛侠12",
    "蜘蛛侠3",
    "鹅",
    "饿",
    "ā",
    "a1",
    "ǜ",
    "ü4",
    "ê̄",
    "ń",
    "😃",
    "𠀀",
    "中\0国",
  ];
  for a in inputs {
    for b in inputs {
      assert_eq!(
        compare(a, b),
        legacy_sort_key(a).cmp(&legacy_sort_key(b)),
        "{a} / {b}"
      );
    }
  }
  // Covers the maximum expansion of every dictionary syllable too.
  for ch in (0..=0x10ffff)
    .filter_map(char::from_u32)
    .filter(|&ch| lookup(ch).is_some())
  {
    let a = ch.to_string();
    assert_eq!(
      compare(&a, "中"),
      legacy_sort_key(&a).cmp(&legacy_sort_key("中")),
      "{a}"
    );
  }
}
