#![deny(clippy::all)]

use std::{cmp::Ordering, convert::TryFrom};

use jieba_rs::Jieba;
use napi::{bindgen_prelude::*, ScopedTask};
use napi_derive::napi;
use once_cell::sync::Lazy;
use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};
use rayon::prelude::*;

#[cfg(not(target_family = "wasm"))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

static JIEBA: Lazy<Jieba> = Lazy::new(Jieba::new);

#[napi(js_name = "PINYIN_STYLE")]
#[derive(Debug, Clone, Copy)]
/// 拼音风格
pub enum PinyinStyle {
  /// 普通风格，不带声调
  Plain = 0,
  /// 带声调的风格
  WithTone = 1,
  /// 声调在各个拼音之后，使用数字1-4表示的风格
  WithToneNum = 2,
  /// 声调在拼音最后，使用数字1-4表示的风格
  WithToneNumEnd = 3,
  /// 首字母风格
  FirstLetter = 4,
}

impl TryFrom<u32> for PinyinStyle {
  type Error = Error;

  fn try_from(value: u32) -> Result<Self> {
    match value {
      0 => Ok(Self::Plain),
      1 => Ok(Self::WithTone),
      2 => Ok(Self::WithToneNum),
      3 => Ok(Self::WithToneNumEnd),
      4 => Ok(Self::FirstLetter),
      _ => Err(Error::new(
        Status::InvalidArg,
        format!("Expected 0|1|2|3|4, but `{value}` provided"),
      )),
    }
  }
}

pub struct AsyncPinyinTask {
  style: PinyinStyle,
  input: Either<String, Buffer>,
  option: PinyinOption,
}

pub enum PinyinData {
  Multi(Vec<Vec<String>>),
  Default(Vec<String>),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinyinOption {
  Default = 0,        // 00
  Multi = 1,          // 01
  SegmentDefault = 2, // 10
  SegmentMulti = 3,   // 11
}

#[napi(object)]
pub struct PinyinConvertOptions {
  pub style: Option<PinyinStyle>,
  pub heteronym: Option<bool>,
  pub segment: Option<bool>,
}

impl From<u8> for PinyinOption {
  fn from(value: u8) -> Self {
    match value {
      0 => Self::Default,
      1 => Self::Multi,
      2 => Self::SegmentDefault,
      3 => Self::SegmentMulti,
      _ => unreachable!(),
    }
  }
}

impl<'task> ScopedTask<'task> for AsyncPinyinTask {
  type Output = PinyinData;
  type JsValue = Array<'task>;

  fn compute(&mut self) -> Result<Self::Output> {
    let input = get_chars_buffer(&self.input);
    match self.option {
      PinyinOption::Default => {
        let input_len = input.len();
        let mut output_py: Vec<String> = Vec::with_capacity(input_len);
        let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_len);
        for c in input.chars() {
          if let Some(py) = c.to_pinyin() {
            if !non_hans_chars.is_empty() {
              output_py.push(non_hans_chars.par_iter().collect::<String>());
              non_hans_chars.clear();
            }
            output_py.push(get_pinyin(py, self.style).to_owned());
          } else {
            non_hans_chars.push(c);
          }
        }
        if !non_hans_chars.is_empty() {
          output_py.push(non_hans_chars.into_par_iter().collect::<String>())
        }
        Ok(PinyinData::Default(output_py))
      }
      PinyinOption::SegmentDefault => {
        let input_words = JIEBA.cut_all(input);
        let input_len = input.len();
        let mut output_py: Vec<String> = Vec::with_capacity(input_len);
        let mut has_pinyin = false;
        let mut non_hans = String::with_capacity(input_len);
        for word in input_words {
          for py in word.to_pinyin().flatten() {
            if !non_hans.is_empty() {
              output_py.push(non_hans.clone());
              non_hans.clear();
            }
            output_py.push(get_pinyin(py, self.style).to_owned());
            has_pinyin = true;
          }
          if !has_pinyin {
            non_hans.push_str(word);
          }
        }
        if !non_hans.is_empty() {
          output_py.push(non_hans);
        }
        Ok(PinyinData::Default(output_py))
      }
      PinyinOption::Multi => {
        let input_chars = input.chars();
        let input_len = input.len();
        let mut output_multi_py: Vec<Vec<String>> = Vec::with_capacity(input_len);
        let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_len);
        for c in input_chars {
          if let Some(multi_py) = c.to_pinyin_multi() {
            if !non_hans_chars.is_empty() {
              output_multi_py.push(vec![if non_hans_chars.len() >= 1024 {
                non_hans_chars.par_iter().collect::<String>()
              } else {
                non_hans_chars.iter().collect::<String>()
              }]);
              non_hans_chars.clear();
            }
            let multi_py_vec = multi_py
              .into_iter()
              .map(|py| get_pinyin(py, self.style).to_owned())
              .collect();

            output_multi_py.push(multi_py_vec);
          } else {
            non_hans_chars.push(c);
          }
        }
        if !non_hans_chars.is_empty() {
          output_multi_py.push(vec![non_hans_chars.into_par_iter().collect::<String>()]);
        }
        Ok(PinyinData::Multi(output_multi_py))
      }
      PinyinOption::SegmentMulti => {
        let input_words = JIEBA.cut_all(input);
        let input_len = input.len();
        let mut output_multi_py: Vec<Vec<String>> = Vec::with_capacity(input_len);
        let mut non_hans = String::with_capacity(input.len());
        for word in input_words {
          let multi_py = word.to_pinyin_multi();
          let mut has_pinyin = false;
          for py in multi_py.flatten() {
            if !non_hans.is_empty() {
              output_multi_py.push(vec![non_hans.clone()]);
              non_hans.clear();
            }
            let multi_py_vec = py
              .into_iter()
              .map(|p| get_pinyin(p, self.style).to_owned())
              .collect();
            output_multi_py.push(multi_py_vec);
            has_pinyin = true;
          }
          if !has_pinyin {
            non_hans.push_str(word);
          }
        }
        if !non_hans.is_empty() {
          output_multi_py.push(vec![non_hans]);
        }
        Ok(PinyinData::Multi(output_multi_py))
      }
    }
  }

  fn resolve(&mut self, env: &'task Env, output: Self::Output) -> Result<Self::JsValue> {
    let js_value = match output {
      PinyinData::Default(arr) => {
        let mut output_arr = env.create_array(arr.len() as u32)?;

        for (index, item) in arr.into_iter().enumerate() {
          output_arr.set_element(index as u32, env.create_string_from_std(item)?)?;
        }

        output_arr
      }
      PinyinData::Multi(arr) => {
        let mut output_arr = env.create_array(arr.len() as u32)?;
        for (index, multi) in arr.into_iter().enumerate() {
          let mut multi_arr = env.create_array(multi.len() as u32)?;
          for (multi_index, item) in multi.into_iter().enumerate() {
            multi_arr.set_element(multi_index as u32, env.create_string_from_std(item)?)?;
          }
          output_arr.set_element(index as u32, multi_arr)?;
        }
        output_arr
      }
    };
    Ok(js_value)
  }
}

#[napi(js_name = "pinyin", ts_return_type = "string[] | string[][]")]
pub fn to_pinyin<'env>(
  env: &'env Env,
  input: Either<String, &'env [u8]>,
  opt: Option<PinyinConvertOptions>,
) -> Result<Array<'env>> {
  let opt = opt.unwrap_or(PinyinConvertOptions {
    style: Some(PinyinStyle::Plain),
    segment: Some(false),
    heteronym: Some(false),
  });
  let option = to_option(opt.segment.unwrap_or(false), opt.heteronym.unwrap_or(false));
  let style = opt.style.unwrap_or(PinyinStyle::Plain);
  let input_str = get_chars(&input)?;
  match option {
    PinyinOption::Default => {
      let mut result_arr = Vec::with_capacity(input_str.len());
      let input_chars = input_str.chars();
      let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_str.len());
      for c in input_chars {
        if let Some(py) = c.to_pinyin() {
          if !non_hans_chars.is_empty() {
            result_arr.push(if non_hans_chars.len() >= 1024 {
              non_hans_chars.par_iter().collect::<String>()
            } else {
              non_hans_chars.iter().collect::<String>()
            });
            non_hans_chars.clear();
          }
          result_arr.push(get_pinyin(py, style).to_owned());
        } else {
          non_hans_chars.push(c);
        }
      }
      if !non_hans_chars.is_empty() {
        result_arr.push(if non_hans_chars.len() >= 1024 {
          non_hans_chars.into_par_iter().collect::<String>()
        } else {
          non_hans_chars.into_iter().collect::<String>()
        });
      }
      Array::from_vec(env, result_arr)
    }
    PinyinOption::SegmentDefault => {
      let mut result_arr = Vec::new();
      let input_words = JIEBA.cut(input_str, false);
      let mut non_hans = String::with_capacity(input_str.len());
      for word in input_words {
        let mut has_pinyin = false;
        for py in word.to_pinyin().flatten() {
          if !non_hans.is_empty() {
            result_arr.push(non_hans.clone());
            non_hans.clear();
          }
          result_arr.push(get_pinyin(py, style).to_owned());
          has_pinyin = true;
        }
        if !has_pinyin {
          non_hans.push_str(word);
        }
      }
      if !non_hans.is_empty() {
        result_arr.push(non_hans);
      }
      Array::from_vec(env, result_arr)
    }
    PinyinOption::Multi => {
      let mut result_arr = Vec::new();
      let input_chars = input_str.chars();
      let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_str.len());
      for c in input_chars {
        if let Some(multi_py) = c.to_pinyin_multi() {
          if !non_hans_chars.is_empty() {
            let buf_arr = vec![non_hans_chars.par_iter().collect::<String>()];
            result_arr.push(buf_arr);
            non_hans_chars.clear();
          }
          let multi_py_vec = multi_py
            .into_iter()
            .map(|py| get_pinyin(py, style).to_owned())
            .collect();
          result_arr.push(multi_py_vec);
        } else {
          non_hans_chars.push(c);
        }
      }
      if !non_hans_chars.is_empty() {
        let buf_arr = vec![non_hans_chars.into_par_iter().collect::<String>()];
        result_arr.push(buf_arr);
      }
      Array::from_vec(env, result_arr)
    }
    PinyinOption::SegmentMulti => {
      let mut result_arr = Vec::new();
      let input_words = JIEBA.cut(input_str, false);
      let mut non_hans = String::with_capacity(input_str.len());
      for word in input_words {
        let multi_py = word.to_pinyin_multi();
        let mut has_pinyin = false;
        for py in multi_py.flatten() {
          if !non_hans.is_empty() {
            let buf_arr = vec![non_hans.clone()];
            result_arr.push(buf_arr);
            non_hans.clear();
          }
          let multi_py_vec = py
            .into_iter()
            .map(|p| get_pinyin(p, style).to_owned())
            .collect();

          result_arr.push(multi_py_vec);
          has_pinyin = true;
        }
        if !has_pinyin {
          non_hans.push_str(word);
        }
      }
      if !non_hans.is_empty() {
        let buf_arr = vec![non_hans];
        result_arr.push(buf_arr);
      }
      Array::from_vec(env, result_arr)
    }
  }
}

fn get_pinyin(input: Pinyin, style: PinyinStyle) -> &'static str {
  match style {
    PinyinStyle::Plain => input.plain(),
    PinyinStyle::WithTone => input.with_tone(),
    PinyinStyle::WithToneNum => input.with_tone_num(),
    PinyinStyle::WithToneNumEnd => input.with_tone_num_end(),
    PinyinStyle::FirstLetter => input.first_letter(),
  }
}

fn get_chars<'a>(input: &'a Either<String, &'a [u8]>) -> Result<&'a str> {
  match input {
    Either::A(input) => Ok(input.as_str()),
    Either::B(input) => Ok(unsafe {
      std::str::from_utf8_unchecked(std::slice::from_raw_parts(input.as_ptr(), input.len()))
    }),
  }
}

fn get_chars_buffer(input: &Either<String, Buffer>) -> &str {
  match input {
    Either::A(input) => input.as_str(),
    Either::B(input) => unsafe { std::str::from_utf8_unchecked(input.as_ref()) },
  }
}

#[napi(ts_return_type = "Promise<string[] | string[][]>")]
pub fn async_pinyin(
  input: Either<String, Buffer>,
  opt: Option<PinyinConvertOptions>,
  signal: Option<AbortSignal>,
) -> Result<AsyncTask<AsyncPinyinTask>> {
  let opt = opt.unwrap_or(PinyinConvertOptions {
    style: Some(PinyinStyle::Plain),
    segment: Some(false),
    heteronym: Some(false),
  });
  let option = to_option(opt.segment.unwrap_or(false), opt.heteronym.unwrap_or(false));

  let task = AsyncPinyinTask {
    style: opt.style.unwrap_or(PinyinStyle::Plain),
    input,
    option,
  };
  Ok(AsyncTask::with_optional_signal(task, signal))
}

fn to_option(need_segment: bool, should_to_multi: bool) -> PinyinOption {
  ((u8::from(need_segment) << 1) | u8::from(should_to_multi)).into()
}

/**
 * 将带声调的拼音转换为数字表示声调的拼音
 * replace pinyin with tone to pinyin with tone number
 *
 * 如  ā 替换为 a1, á 替换为 a2, ǎ 替换为 a3, à 替换为 a4
 * e.g. ā -> a1, á -> a2, ǎ -> a3, à -> a4
 */
fn pinyin_tone_to_number(input: &str) -> String {
  let replacements = [
    ('ā', "a1"),
    ('á', "a2"),
    ('ǎ', "a3"),
    ('à', "a4"),
    ('ē', "e1"),
    ('é', "e2"),
    ('ě', "e3"),
    ('è', "e4"),
    ('ī', "i1"),
    ('í', "i2"),
    ('ǐ', "i3"),
    ('ì', "i4"),
    ('ō', "o1"),
    ('ó', "o2"),
    ('ǒ', "o3"),
    ('ò', "o4"),
    ('ū', "u1"),
    ('ú', "u2"),
    ('ǔ', "u3"),
    ('ù', "u4"),
    ('ǖ', "ü1"),
    ('ǘ', "ü2"),
    ('ǚ', "ü3"),
    ('ǜ', "ü4"),
  ];

  let mut output = input.to_string();
  for &(character, replacement) in &replacements {
    output = output.replace(character, replacement);
  }

  output
}

/**
 * 比较两个带声调的拼音字符串
 * compare two pinyin with tone strings
 */
fn cmp_pinyin_with_tone(input_a: &str, input_b: &str) -> Ordering {
  let pinyin_a = pinyin_tone_to_number(input_a);
  let pinyin_b = pinyin_tone_to_number(input_b);
  pinyin_a.cmp(&pinyin_b)
}

/**
 * 将给定的字符串转换为带声调的拼音字符串
 * convert the given string to pinyin string with tone
 */
fn str_to_pinyin_with_tone(input: &str) -> String {
  input
    .chars()
    .map(|c| {
      c.to_pinyin()
        .map_or(c.to_string(), |p| p.with_tone().to_string())
    })
    .collect::<String>()
}

#[napi]
pub fn compare(input_a: String, input_b: String) -> Result<i32> {
  let a_with_tone = str_to_pinyin_with_tone(&input_a);
  let b_with_tone = str_to_pinyin_with_tone(&input_b);

  Ok(cmp_pinyin_with_tone(&a_with_tone, &b_with_tone) as _)
}

#[cfg(test)]
mod test {

  use super::{compare, to_option, PinyinOption};
  #[test]
  fn convert_option() {
    assert_eq!(to_option(false, false), PinyinOption::Default);
    assert_eq!(to_option(false, true), PinyinOption::Multi);
    assert_eq!(to_option(true, false), PinyinOption::SegmentDefault);
    assert_eq!(to_option(true, true), PinyinOption::SegmentMulti);
  }

  #[test]
  fn do_compare() {
    let smaller = String::from("蜘蛛侠1");
    let middle = String::from("蜘蛛侠12");
    let greater = String::from("蜘蛛侠3");
    let empty = String::from("");

    assert_eq!(compare(smaller.clone(), middle.clone()).unwrap(), -1);
    assert_eq!(compare(middle.clone(), middle.clone()).unwrap(), 0);
    assert_eq!(compare(middle.clone(), greater.clone()).unwrap(), -1);
    assert_eq!(compare(greater.clone(), middle.clone()).unwrap(), 1);
    assert_eq!(compare(empty.clone(), empty.clone()).unwrap(), 0);
  }
}
