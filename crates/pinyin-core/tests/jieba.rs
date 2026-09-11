#![cfg(feature = "jieba")]

use pinyin_core::{jieba, Style};
use std::sync::LazyLock;

static SEGMENTER: LazyLock<jieba::Jieba> = LazyLock::new(jieba::Jieba::new);

#[test]
fn boundaries_guide_overlapping_phrases_without_discarding_fallbacks() {
  let mut segmenter = jieba::Jieba::empty();
  assert_eq!(
    jieba::pinyin("重庆", Style::Plain, &segmenter, false, " "),
    "chong qing"
  );
  // 一号 and 号叫 overlap. A caller's segmentation dictionary guides which
  // phrase wins, while the same pronunciation data is used in both cases.
  let original = pinyin_core::pinyin("一号叫", Style::Tone, true, " ");
  assert_eq!(original, "yī hào jiào");
  segmenter.add_word("号叫", Some(100), None);
  assert_eq!(
    jieba::pinyin("一号叫", Style::Tone, &segmenter, false, " "),
    "yī háo jiào"
  );
  segmenter.clear();
  assert_eq!(
    jieba::pinyin("一号叫", Style::Tone, &segmenter, false, " "),
    original
  );
  segmenter.add_word("重庆银行", Some(100), None);
  assert_eq!(
    jieba::pinyin("重庆银行", Style::Plain, &segmenter, false, " "),
    "chong qing yin hang"
  );
}

#[test]
fn contextual_readings_are_selected_after_default_jieba_segmentation() {
  assert_eq!(
    jieba::pinyin("重庆银行音乐", Style::Tone, &SEGMENTER, false, " "),
    "chóng qìng yín háng yīn yuè"
  );
}

#[test]
fn unicode_ranges_and_non_han_grouping_survive_both_hmm_modes() {
  for input in [
    "",
    "ASCII\0\r\n",
    "𠮷👨‍👩‍👧‍👦重庆\u{301} \0🙂银行\r\nZ",
    "A中\"\\\t\u{2028}文",
  ] {
    for hmm in [false, true] {
      let tokens: Vec<_> = jieba::tokens(input, &SEGMENTER, hmm).collect();
      let reconstructed: String = tokens.iter().map(|token| &input[token.range()]).collect();
      assert_eq!(reconstructed, input);
      assert!(!tokens
        .windows(2)
        .any(|pair| pair.iter().all(|t| t.syllable().is_none())));
      for style in [
        Style::Plain,
        Style::Tone,
        Style::ToneNumber,
        Style::ToneNumberEnd,
        Style::FirstLetter,
      ] {
        let expected = tokens
          .iter()
          .map(|token| token.text(input, style))
          .collect::<Vec<_>>()
          .join("|🙂\0");
        assert_eq!(
          jieba::pinyin(input, style, &SEGMENTER, hmm, "|🙂\0"),
          expected
        );
        let mut output = String::from("prefix:");
        jieba::write_pinyin(input, style, &SEGMENTER, hmm, "|🙂\0", &mut output);
        assert_eq!(output, format!("prefix:{expected}"));
        #[cfg(feature = "utf16")]
        for separator in ["", " ", "|🙂\0"] {
          let expected = tokens
            .iter()
            .map(|token| token.text(input, style))
            .collect::<Vec<_>>()
            .join(separator);
          assert_eq!(
            jieba::pinyin_utf16(input, style, &SEGMENTER, hmm, separator),
            expected.encode_utf16().collect::<Vec<_>>()
          );
        }
      }
    }
  }
}

#[test]
fn word_boundaries_preserve_known_cross_word_pronunciations() {
  for input in ["划分为", "统称为", "可以划分为纯文学", "书籍文献统称为文学"]
  {
    assert!(SEGMENTER
      .cut(input, false)
      .iter()
      .any(|word| word.word == "为"));
    assert_eq!(
      jieba::pinyin(input, Style::Tone, &SEGMENTER, false, " "),
      pinyin_core::pinyin(input, Style::Tone, true, " ")
    );
  }
}

#[test]
fn weighted_trie_matches_independent_full_dictionary_solver() {
  let dictionary: Vec<_> = include_str!("../data/phrases.tsv")
    .lines()
    .filter(|line| !line.starts_with('#') && !line.is_empty())
    .map(|line| {
      let (word, reading) = line.split_once('\t').unwrap();
      (word.chars().collect::<Vec<_>>(), reading.replace(' ', ""))
    })
    .collect();
  let chunks = [
    "一号叫",
    "重庆银行",
    "划分为",
    "统称为",
    "重新处理",
    "音乐会",
    "𠮷🙂",
    "长大",
    "快乐",
    "重要",
    "\0",
    "行长",
  ];
  let mut seed = 123456789u32;
  for _ in 0..40 {
    let input: String = (0..16)
      .map(|_| {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        chunks[(seed >> 16) as usize % chunks.len()]
      })
      .collect();
    let chars: Vec<_> = input.chars().collect();
    for hmm in [false, true] {
      let words = SEGMENTER.cut(&input, hmm);
      let mut costs = vec![0.0; chars.len() + 1];
      let mut choices = vec![None; chars.len()];
      for i in (0..chars.len()).rev() {
        costs[i] = 13.0 + costs[i + 1];
        for (index, (word, _)) in dictionary.iter().enumerate() {
          if chars[i..].starts_with(word) {
            let end = i + word.len();
            let crossing = words.iter().filter(|w| w.end > i && w.end < end).count();
            let score = 7.698970004336019 + crossing as f64 + costs[end];
            let longer =
              choices[i].is_none_or(|previous: usize| word.len() > dictionary[previous].0.len());
            if score < costs[i] || (score == costs[i] && longer) {
              costs[i] = score;
              choices[i] = Some(index);
            }
          }
        }
      }
      let mut expected = String::new();
      let mut i = 0;
      while i < chars.len() {
        if let Some(index) = choices[i] {
          expected.push_str(&dictionary[index].1);
          i += dictionary[index].0.len();
        } else {
          if let Some(reading) = pinyin_core::lookup(chars[i]) {
            expected.push_str(reading.text(Style::Tone));
          } else {
            expected.push(chars[i]);
          }
          i += 1;
        }
      }
      assert_eq!(
        jieba::pinyin(&input, Style::Tone, &SEGMENTER, hmm, ""),
        expected,
        "{input:?}"
      );
    }
  }
}
