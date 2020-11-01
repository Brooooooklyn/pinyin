#[macro_use]
extern crate napi_derive;

use std::convert::{TryFrom, TryInto};

use jieba_rs::Jieba;
use napi::*;
use once_cell::sync::OnceCell;
use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};

#[cfg(all(unix, not(target_env = "musl")))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(windows)]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
  should_to_multi: bool,
  need_segment: bool,
}

enum PinyinData {
  Multi(Vec<Vec<String>>),
  Default(Vec<String>),
}

impl Task for AsyncPinyinTask {
  type Output = PinyinData;
  type JsValue = JsObject;

  fn compute(&mut self) -> Result<Self::Output> {
    let jieba = JIEBA.get_or_init(|| Jieba::new());
    let words = if self.need_segment {
      jieba.cut(self.input.as_str(), false)
    } else {
      vec![self.input.as_str()]
    };

    let mut has_pinyin = false;

    if self.should_to_multi {
      let mut buf: Vec<Vec<String>> = Vec::new();
      for word in words {
        let pinyin_iter = word.to_pinyin_multi();
        let (_, max) = pinyin_iter.size_hint();
        let mut result_arr: Vec<Vec<String>> = Vec::with_capacity(max.unwrap_or(word.len()));
        for pinyin in pinyin_iter {
          if let Some(multi) = pinyin {
            let mut multi_arr = Vec::with_capacity(multi.count());
            for pinyin in multi {
              multi_arr.push(get_pinyin(pinyin, self.style).to_owned());
            }
            result_arr.push(multi_arr);
            has_pinyin = true;
          }
        }
        if has_pinyin {
          buf.extend_from_slice(result_arr.as_slice());
        } else {
          buf.push(vec![self.input.clone()])
        }
      }
      Ok(PinyinData::Multi(buf))
    } else {
      let mut buf = Vec::new();
      for word in words {
        let pinyin_iter = word.to_pinyin();
        let (_, max) = pinyin_iter.size_hint();
        let mut result_arr = Vec::with_capacity(max.unwrap_or(word.len()));
        for pinyin in pinyin_iter {
          if let Some(pinyin) = pinyin {
            result_arr.push(get_pinyin(pinyin, self.style).to_owned());
            has_pinyin = true;
          }
        }
        buf.extend_from_slice(result_arr.as_slice());
      }
      if !has_pinyin {
        buf.push(self.input.clone());
      }
      Ok(PinyinData::Default(buf))
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
  let jieba = JIEBA.get_or_init(|| Jieba::new());
  let input_string = ctx.get::<JsString>(0)?;
  let input_str = input_string.into_utf8()?;

  let should_to_multi = ctx.get::<JsBoolean>(1)?.get_value()?;

  let style = {
    let style: u32 = ctx.get::<JsNumber>(2)?.try_into()?;
    style.try_into()?
  };

  let need_segment = ctx.get::<JsBoolean>(3)?.get_value()?;

  let mut result_arr = ctx.env.create_array()?;
  let mut index = 0;

  let words = if need_segment {
    jieba.cut(input_str.as_str()?, false)
  } else {
    vec![input_str.as_str()?]
  };

  let mut has_pinyin = false;

  if should_to_multi {
    for word in words {
      let pinyin_iter = word.to_pinyin_multi();
      for pinyin in pinyin_iter {
        if let Some(multi) = pinyin {
          let mut multi_arr = ctx.env.create_array()?;
          let mut multi_index = 0;
          for pinyin in multi {
            multi_arr.set_element(
              multi_index,
              ctx.env.create_string(get_pinyin(pinyin, style))?,
            )?;
            multi_index += 1;
          }
          result_arr.set_element(index, multi_arr)?;
          index += 1;
          has_pinyin = true;
        }
      }
    }
    if !has_pinyin {
      let mut multi_arr = ctx.env.create_array_with_length(1)?;
      multi_arr.set_element(0, input_string)?;
      result_arr.set_element(index, multi_arr)?;
    }
  } else {
    for word in words {
      let pinyin_iter = word.to_pinyin();
      for pinyin in pinyin_iter {
        if let Some(pinyin) = pinyin {
          result_arr.set_element(index, ctx.env.create_string(get_pinyin(pinyin, style))?)?;
          index += 1;
          has_pinyin = true;
        }
      }
    }
    if !has_pinyin {
      result_arr.set_element(0, input_string)?;
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
  let task = AsyncPinyinTask {
    input,
    style,
    should_to_multi,
    need_segment,
  };
  ctx.env.spawn(task).map(|v| v.promise_object())
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
