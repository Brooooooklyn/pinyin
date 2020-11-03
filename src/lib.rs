#[macro_use]
extern crate napi_derive;

use std::convert::{TryFrom, TryInto};

use jieba_rs::Jieba;
use napi::*;
use once_cell::sync::OnceCell;
use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};
use rayon::prelude::*;

#[cfg(all(unix, not(target_env = "musl")))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(windows)]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(not(test))]
register_module!(pinyin, init);

static JIEBA: OnceCell<Jieba> = OnceCell::new();

#[derive(Clone, Copy, Debug)]
enum PinyinStyle {
  // 普通风格，不带声调
  Plain = 0,
  // 带声调的风格
  WithTone = 1,
  // 声调在各个拼音之后，使用数字1-4表示的风格
  WithToneNum = 2,
  // 声调在拼音最后，使用数字1-4表示的风格
  WithToneNumEnd = 3,
  // 首字母风格
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

#[cfg(not(test))]
fn init(module: &mut Module) -> Result<()> {
  let _ = JIEBA.get_or_init(|| Jieba::new());

  let mut pinyin_style = module.env.create_object()?;

  pinyin_style.define_properties(&vec![
    Property::new(&module.env, "Plain")?
      .with_value(module.env.create_uint32(PinyinStyle::Plain as _)?)
      .with_property_attributes(PropertyAttributes::Enumerable),
    Property::new(&module.env, "WithTone")?
      .with_value(module.env.create_uint32(PinyinStyle::WithTone as _)?)
      .with_property_attributes(PropertyAttributes::Enumerable),
    Property::new(&module.env, "WithToneNum")?
      .with_value(module.env.create_uint32(PinyinStyle::WithToneNum as _)?)
      .with_property_attributes(PropertyAttributes::Enumerable),
    Property::new(&module.env, "WithToneNumEnd")?
      .with_value(module.env.create_uint32(PinyinStyle::WithToneNumEnd as _)?)
      .with_property_attributes(PropertyAttributes::Enumerable),
    Property::new(&module.env, "FirstLetter")?
      .with_value(module.env.create_uint32(PinyinStyle::FirstLetter as _)?)
      .with_property_attributes(PropertyAttributes::Enumerable),
  ])?;

  module
    .exports
    .set_named_property("PINYIN_STYLE", pinyin_style)?;

  module.create_named_method("pinyin", to_pinyin)?;

  module.create_named_method("asyncPinyin", async_pinyin)?;

  module.create_named_method("compare", compare_pinyin)?;
  Ok(())
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

#[derive(Debug, PartialEq, Eq)]
#[repr(u8)]
enum PinyinOption {
  Default = 0,        // 00
  Multi = 1,          // 01
  SegmentDefault = 2, // 10
  SegmentMulti = 3,   // 11
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
  type JsValue = JsObject;

  fn compute(&mut self) -> Result<Self::Output> {
    match self.option {
      PinyinOption::Default => {
        let input_chars = self.input.chars();
        let input_len = self.input.len();
        let mut output_py: Vec<String> = Vec::with_capacity(input_len);
        let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_len);
        for c in input_chars {
          if let Some(py) = c.to_pinyin() {
            if non_hans_chars.len() > 0 {
              output_py.push(non_hans_chars.par_iter().collect::<String>());
              non_hans_chars.clear();
            }
            output_py.push(get_pinyin(py, self.style).to_owned());
          } else {
            non_hans_chars.push(c);
          }
        }
        if non_hans_chars.len() > 0 {
          output_py.push(non_hans_chars.into_par_iter().collect::<String>())
        }
        Ok(PinyinData::Default(output_py))
      }
      PinyinOption::SegmentDefault => {
        let jieba = JIEBA.get_or_init(|| Jieba::new());
        let input_words = jieba.cut_all(self.input.as_str());
        let input_len = self.input.len();
        let mut output_py: Vec<String> = Vec::with_capacity(input_len);
        let mut has_pinyin = false;
        let mut non_hans = String::with_capacity(input_len);
        for word in input_words {
          for maybe_py in word.to_pinyin() {
            if let Some(py) = maybe_py {
              if non_hans.len() > 0 {
                output_py.push(non_hans.clone());
                non_hans.clear();
              }
              output_py.push(get_pinyin(py, self.style).to_owned());
              has_pinyin = true;
            }
          }
          if !has_pinyin {
            non_hans.push_str(word);
          }
        }
        if non_hans.len() > 0 {
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
            if non_hans_chars.len() > 0 {
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
        if non_hans_chars.len() > 0 {
          output_multi_py.push(vec![non_hans_chars.into_par_iter().collect::<String>()]);
        }
        Ok(PinyinData::Multi(output_multi_py))
      }
      PinyinOption::SegmentMulti => {
        let jieba = JIEBA.get_or_init(|| Jieba::new());
        let input_words = jieba.cut_all(self.input.as_str());
        let input_len = self.input.len();
        let mut output_multi_py: Vec<Vec<String>> = Vec::with_capacity(input_len);
        let mut non_hans = String::with_capacity(self.input.len());
        for word in input_words {
          let multi_py = word.to_pinyin_multi();
          let mut has_pinyin = false;
          for maybe_py in multi_py {
            if let Some(py) = maybe_py {
              if non_hans.len() > 0 {
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
          }
          if !has_pinyin {
            non_hans.push_str(word);
          }
        }
        if non_hans.len() > 0 {
          output_multi_py.push(vec![non_hans]);
        }
        Ok(PinyinData::Multi(output_multi_py))
      }
    }
  }

  fn resolve(self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
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

#[js_function(4)]
fn to_pinyin(ctx: CallContext) -> Result<JsObject> {
  let input_string = ctx.get::<JsString>(0)?;
  let input_str = input_string.into_utf8()?;

  let should_to_multi = ctx.get::<JsBoolean>(1)?.get_value()?;

  let style: PinyinStyle = {
    let style: u32 = ctx.get::<JsNumber>(2)?.try_into()?;
    style.try_into()?
  };

  let need_segment = ctx.get::<JsBoolean>(3)?.get_value()?;

  let mut result_arr = ctx.env.create_array()?;
  let mut index = 0;

  let option = to_option(need_segment, should_to_multi);

  match option {
    PinyinOption::Default => {
      let input_str = input_str.as_str()?;
      let input_chars = input_str.chars();
      let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_str.len());
      for c in input_chars {
        if let Some(py) = c.to_pinyin() {
          if non_hans_chars.len() > 0 {
            result_arr.set_element(
              index,
              ctx
                .env
                .create_string_from_std(non_hans_chars.par_iter().collect::<String>())?,
            )?;
            non_hans_chars.clear();
            index += 1;
          }
          result_arr.set_element(index, ctx.env.create_string(get_pinyin(py, style))?)?;
          index += 1;
        } else {
          non_hans_chars.push(c);
        }
      }
      if non_hans_chars.len() > 0 {
        result_arr.set_element(
          index,
          ctx
            .env
            .create_string_from_std(non_hans_chars.into_par_iter().collect::<String>())?,
        )?;
      }
    }
    PinyinOption::SegmentDefault => {
      let jieba = JIEBA.get_or_init(|| Jieba::new());
      let input_str = input_str.as_str()?;
      let input_words = jieba.cut(input_str, false);
      let mut non_hans = String::with_capacity(input_str.len());
      for word in input_words {
        let mut has_pinyin = false;
        for maybe_py in word.to_pinyin() {
          if let Some(py) = maybe_py {
            if non_hans.len() > 0 {
              result_arr.set_element(index, ctx.env.create_string(non_hans.as_str())?)?;
              non_hans.clear();
              index += 1;
            }
            result_arr.set_element(index, ctx.env.create_string(get_pinyin(py, style))?)?;
            has_pinyin = true;
            index += 1;
          }
        }
        if !has_pinyin {
          non_hans.push_str(word);
        }
      }
      if non_hans.len() > 0 {
        result_arr.set_element(index, ctx.env.create_string_from_std(non_hans)?)?;
      }
    }
    PinyinOption::Multi => {
      let input_str = input_str.as_str()?;
      let input_chars = input_str.chars();
      let mut non_hans_chars: Vec<char> = Vec::with_capacity(input_str.len());
      for c in input_chars {
        if let Some(multi_py) = c.to_pinyin_multi() {
          if non_hans_chars.len() > 0 {
            let mut buf_arr = ctx.env.create_array_with_length(1)?;
            buf_arr.set_element(
              0,
              ctx
                .env
                .create_string_from_std(non_hans_chars.par_iter().collect::<String>())?,
            )?;
            result_arr.set_element(index, buf_arr)?;
            non_hans_chars.clear();
            index += 1;
          }
          let mut multi_py_vec = ctx.env.create_array_with_length(multi_py.count())?;
          let mut multi_py_index = 0;
          for py in multi_py {
            multi_py_vec.set_element(
              multi_py_index,
              ctx.env.create_string(get_pinyin(py, style))?,
            )?;
            multi_py_index += 1;
          }
          result_arr.set_element(index, multi_py_vec)?;
          index += 1;
        } else {
          non_hans_chars.push(c);
        }
      }
      if non_hans_chars.len() > 0 {
        let mut buf_arr = ctx.env.create_array_with_length(1)?;
        buf_arr.set_element(
          0,
          ctx
            .env
            .create_string_from_std(non_hans_chars.into_par_iter().collect::<String>())?,
        )?;
        result_arr.set_element(index, buf_arr)?;
      }
    }
    PinyinOption::SegmentMulti => {
      let jieba = JIEBA.get_or_init(|| Jieba::new());
      let input_str = input_str.as_str()?;
      let input_words = jieba.cut(input_str, false);
      let mut non_hans = String::with_capacity(input_str.len());
      for word in input_words {
        let multi_py = word.to_pinyin_multi();
        let mut has_pinyin = false;
        for maybe_py in multi_py {
          if let Some(py) = maybe_py {
            if non_hans.len() > 0 {
              let mut buf_arr = ctx.env.create_array_with_length(1)?;
              buf_arr.set_element(0, ctx.env.create_string(non_hans.as_str())?)?;
              result_arr.set_element(index, buf_arr)?;
              non_hans.clear();
              index += 1;
            }
            let mut multi_py_vec = ctx.env.create_array_with_length(py.count())?;
            let mut multi_py_index = 0;
            for p in py {
              multi_py_vec
                .set_element(multi_py_index, ctx.env.create_string(get_pinyin(p, style))?)?;
              multi_py_index += 1;
            }
            result_arr.set_element(index, multi_py_vec)?;
            index += 1;
            has_pinyin = true;
          }
        }
        if !has_pinyin {
          non_hans.push_str(word);
        }
      }
      if non_hans.len() > 0 {
        let mut buf_arr = ctx.env.create_array_with_length(1)?;
        buf_arr.set_element(0, ctx.env.create_string_from_std(non_hans)?)?;
        result_arr.set_element(index, buf_arr)?;
      }
    }
  };

  Ok(result_arr)
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

#[js_function(4)]
fn async_pinyin(ctx: CallContext) -> Result<JsObject> {
  let input = ctx.get::<JsString>(0)?.into_utf8()?.as_str()?.to_owned();
  let should_to_multi = ctx.get::<JsBoolean>(1)?.get_value()?;

  let style = {
    let style: u32 = ctx.get::<JsNumber>(2)?.try_into()?;
    style.try_into()?
  };
  let need_segment = ctx.get::<JsBoolean>(3)?.get_value()?;
  let option = to_option(need_segment, should_to_multi);

  let task = AsyncPinyinTask {
    input,
    style,
    option: option.into(),
  };
  ctx.env.spawn(task).map(|v| v.promise_object())
}

#[inline(always)]
fn to_option(need_segment: bool, should_to_multi: bool) -> PinyinOption {
  (u8::from(need_segment) << 1 | u8::from(should_to_multi)).into()
}

#[js_function(2)]
fn compare_pinyin(ctx: CallContext) -> Result<JsNumber> {
  let input_a = ctx.get::<JsString>(0)?.into_utf8()?;
  let input_b = ctx.get::<JsString>(1)?.into_utf8()?;
  let input_a_str = input_a.as_str()?;
  let input_b_str = input_b.as_str()?;
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
  ctx.env.create_int32(pinyin_a.cmp(pinyin_b) as _)
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
