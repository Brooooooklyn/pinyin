fn main() {
  let mut original = original::Jieba::new();
  let mut patched = patched::Jieba::new();
  let mut cases = vec![
    include_str!("literature.txt").chars().cycle().take(100000).collect::<String>(),
    "中".repeat(32) + &"API +#&._%-word ".repeat(10000),
  ];
  let alphabet: Vec<char> = "重庆银行音乐划分为一号叫中文测试abcXYZ019+#&._%- \r\n\0é🙂\u{3400}\u{4dbf}\u{4dc0}\u{4dff}\u{4e00}\u{9fff}\u{a000}\u{f900}\u{faff}𠀀𠮷".chars().collect();
  let mut seed = 123456789u32;
  for n in 0..1000 {
    let text = (0..n % 300).map(|_| {
      seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
      alphabet[(seed >> 16) as usize % alphabet.len()]
    }).collect();
    cases.push(text);
  }
  for n in 0..130 {
    for suffix in ["", "é", "🙂", "\u{4dc0}", "\0", "\u{3400}"] {
      cases.push("中".repeat(n) + suffix + &"国".repeat(n));
    }
  }
  let mut count = 0;
  for custom in [false, true] {
    if custom {
      original.clear(); patched.clear();
      for word in ["重庆银行", "号叫", "A重庆", "划分为", "音乐会"] {
        original.add_word(word, Some(100), None);
        patched.add_word(word, Some(100), None);
      }
    }
    for text in &cases {
      for hmm in [false, true] {
        let a = original.cut(text, hmm);
        let b = patched.cut(text, hmm);
        assert_eq!(a.len(), b.len(), "{text:?}");
        for (a, b) in a.iter().zip(&b) {
          assert_eq!((a.word, a.start, a.end, a.byte_start, a.byte_end), (b.word, b.start, b.end, b.byte_start, b.byte_end), "{text:?}");
        }
        count += 1;
      }
    }
  }
  println!("{{\"segmentations_compared\":{count},\"hmm_modes\":[false,true],\"dictionaries\":[\"default\",\"custom\"]}}");
}
