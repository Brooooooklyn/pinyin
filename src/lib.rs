#![deny(clippy::all)]

#[macro_use]
extern crate napi_derive;

use std::convert::TryFrom;

use jieba_rs::Jieba;
use napi::bindgen_prelude::*;
use once_cell::sync::OnceCell;
use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};
use rayon::prelude::*;

#[cfg(all(
  any(windows, unix),
  target_arch = "x86_64",
  not(target_env = "musl"),
  not(debug_assertions)
))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

static JIEBA: OnceCell<Jieba> = OnceCell::new();

#[napi(js_name = "PINYIN_STYLE")]
#[derive(Debug)]
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
        format!("Expected 0|1|2|3|4, but `{}` provided", value),
      )),
    }
  }
}

struct AsyncPinyinTask {
  style: PinyinStyle,
  input: String,
  option: PinyinOption,
}

enum PinyinData {
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

impl Task for AsyncPinyinTask {
  type Output = PinyinData;
  type JsValue = napi::JsObject;

  fn compute(&mut self) -> Result<Self::Output> {
    match self.option {
      PinyinOption::Default => {
        let input_chars = self.input.chars();
        let input_len = self.input.len();
        let mut output_py: Vec<String> = Vec::with_capacity(input_len);
        let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_len);
        for c in input_chars {
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
        let jieba = JIEBA.get_or_init(Jieba::new);
        let input_words = jieba.cut_all(self.input.as_str());
        let input_len = self.input.len();
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
        let input_chars = self.input.chars();
        let input_len = self.input.len();
        let mut output_multi_py: Vec<Vec<String>> = Vec::with_capacity(input_len);
        let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_len);
        for c in input_chars {
          if let Some(multi_py) = c.to_pinyin_multi() {
            if !non_hans_chars.is_empty() {
              output_multi_py.push(vec![non_hans_chars.par_iter().collect::<String>()]);
              non_hans_chars.clear();
            }
            let mut multi_py_vec = Vec::with_capacity(multi_py.count());
            for py in multi_py {
              multi_py_vec.push(get_pinyin(py, self.style).to_owned());
            }
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
        let jieba = JIEBA.get_or_init(Jieba::new);
        let input_words = jieba.cut_all(self.input.as_str());
        let input_len = self.input.len();
        let mut output_multi_py: Vec<Vec<String>> = Vec::with_capacity(input_len);
        let mut non_hans = String::with_capacity(self.input.len());
        for word in input_words {
          let multi_py = word.to_pinyin_multi();
          let mut has_pinyin = false;
          for py in multi_py.flatten() {
            if !non_hans.is_empty() {
              output_multi_py.push(vec![non_hans.clone()]);
              non_hans.clear();
            }
            let mut multi_py_vec = Vec::with_capacity(py.count());
            for p in py {
              multi_py_vec.push(get_pinyin(p, self.style).to_owned());
            }
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

  fn resolve(&mut self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
    let js_value = match output {
      PinyinData::Default(arr) => {
        let mut output_arr = env.create_array_with_length(arr.len())?;

        for (index, item) in arr.into_iter().enumerate() {
          output_arr.set_element(index as u32, env.create_string_from_std(item)?)?;
        }

        output_arr
      }
      PinyinData::Multi(arr) => {
        let mut output_arr = env.create_array_with_length(arr.len())?;
        for (index, multi) in arr.into_iter().enumerate() {
          let mut multi_arr = env.create_array_with_length(multi.len())?;
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
fn to_pinyin(env: Env, input_str: String, opt: Option<PinyinConvertOptions>) -> Result<Array> {
  let opt = opt.unwrap_or(PinyinConvertOptions {
    style: Some(PinyinStyle::Plain),
    segment: Some(false),
    heteronym: Some(false),
  });
  let option = to_option(opt.segment.unwrap_or(false), opt.heteronym.unwrap_or(false));
  let style = opt.style.unwrap_or(PinyinStyle::Plain);

  match option {
    PinyinOption::Default => {
      let mut result_arr = Vec::new();
      let input_chars = input_str.chars();
      let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_str.len());
      for c in input_chars {
        if let Some(py) = c.to_pinyin() {
          if !non_hans_chars.is_empty() {
            result_arr.push(non_hans_chars.par_iter().collect::<String>());
            non_hans_chars.clear();
          }
          result_arr.push(get_pinyin(py, style).to_owned());
        } else {
          non_hans_chars.push(c);
        }
      }
      if !non_hans_chars.is_empty() {
        result_arr.push(non_hans_chars.into_par_iter().collect::<String>());
      }
      Array::from_vec(&env, result_arr)
    }
    PinyinOption::SegmentDefault => {
      let mut result_arr = Vec::new();
      let jieba = JIEBA.get_or_init(Jieba::new);
      let input_words = jieba.cut(input_str.as_str(), false);
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
      Array::from_vec(&env, result_arr)
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
          let mut multi_py_vec = Vec::with_capacity(multi_py.count());
          for py in multi_py.into_iter() {
            multi_py_vec.push(get_pinyin(py, style).to_owned());
          }
          result_arr.push(multi_py_vec);
        } else {
          non_hans_chars.push(c);
        }
      }
      if !non_hans_chars.is_empty() {
        let buf_arr = vec![non_hans_chars.into_par_iter().collect::<String>()];
        result_arr.push(buf_arr);
      }
      Array::from_vec(&env, result_arr)
    }
    PinyinOption::SegmentMulti => {
      let mut result_arr = Vec::new();
      let jieba = JIEBA.get_or_init(Jieba::new);
      let input_words = jieba.cut(input_str.as_str(), false);
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
          let mut multi_py_vec = Vec::with_capacity(py.count());
          for p in py.into_iter() {
            multi_py_vec.push(get_pinyin(p, style).to_owned());
          }
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
      Array::from_vec(&env, result_arr)
    }
  }
}

#[inline(always)]
fn get_pinyin(input: Pinyin, style: PinyinStyle) -> &'static str {
  match style {
    PinyinStyle::Plain => input.plain(),
    PinyinStyle::WithTone => input.with_tone(),
    PinyinStyle::WithToneNum => input.with_tone_num(),
    PinyinStyle::WithToneNumEnd => input.with_tone_num_end(),
    PinyinStyle::FirstLetter => input.first_letter(),
  }
}

#[napi(ts_return_type = "Promise<string[] | string[][]>")]
fn async_pinyin(
  input: String,
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

#[inline(always)]
fn to_option(need_segment: bool, should_to_multi: bool) -> PinyinOption {
  (u8::from(need_segment) << 1 | u8::from(should_to_multi)).into()
}

#[napi]
fn compare(input_a: String, input_b: String) -> Result<i32> {
  let input_a_str = input_a.as_str();
  let input_b_str = input_b.as_str();
  let pinyin_a = input_a_str
    .to_pinyin()
    .next()
    .and_then(|p| p)
    .map(Pinyin::with_tone)
    .unwrap_or(input_a_str);

  let pinyin_b = input_b_str
    .to_pinyin()
    .next()
    .and_then(|p| p)
    .map(Pinyin::with_tone)
    .unwrap_or(input_a_str);
  Ok(pinyin_a.cmp(pinyin_b) as _)
}

#[cfg(test)]
mod test {

  use super::{to_option, PinyinOption};
  #[test]
  fn convert_option() {
    assert_eq!(to_option(false, false), PinyinOption::Default);
    assert_eq!(to_option(false, true), PinyinOption::Multi);
    assert_eq!(to_option(true, false), PinyinOption::SegmentDefault);
    assert_eq!(to_option(true, true), PinyinOption::SegmentMulti);
  }
}
