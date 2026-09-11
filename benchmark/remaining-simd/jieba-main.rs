use std::{hint::black_box, time::{Duration, Instant}};
fn main() {
  let jieba = jieba_rs::Jieba::new();
  let literature = include_str!("literature.txt").chars().cycle().take(100000).collect::<String>();
  let fixtures = [
    ("literature", literature),
    ("mixed", "中国 API é🙂 音乐，2026! ".repeat(5000)),
    ("long-ascii", "中".repeat(32) + &" api=value ".repeat(10000)),
  ];
  for text in ["", "重庆银行音乐", "一号叫划分为", "中\0é🙂\u{4dc0}\u{3400}\u{f900}𠀀\r\nAPI+#%_é中"] {
    for hmm in [false, true] {
      jieba_rs::research_classifier::set(false);
      let expected = jieba.cut(text, hmm);
      jieba_rs::research_classifier::set(true);
      let actual = jieba.cut(text, hmm);
      assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
    }
  }
  for (name, text) in fixtures {
    jieba_rs::research_classifier::set(false);
    let expected = jieba.cut(&text, false);
    jieba_rs::research_classifier::set(true);
    assert_eq!(format!("{expected:?}"), format!("{:?}", jieba.cut(&text, false)));
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..7 {
      for j in 0..2 {
        let mode = (round+j)%2;
        jieba_rs::research_classifier::set(mode != 0);
        let start = Instant::now();
        let mut n = 0;
        while start.elapsed() < Duration::from_millis(75) {
          black_box(jieba.cut(black_box(&text), false)); n += 1;
        }
        samples[mode].push(start.elapsed().as_secs_f64()*1e6 / n as f64);
      }
    }
    println!("{{\"fixture\":\"{name}\",\"samples_us\":{samples:?}}}");
  }
}
