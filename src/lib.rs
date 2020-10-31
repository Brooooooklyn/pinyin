#[macro_use]
extern crate napi_derive;

use std::convert::{TryFrom, TryInto};

use napi::*;
use pinyin::{Pinyin, ToPinyin, ToPinyinMulti};

#[cfg(all(unix, not(target_env = "musl")))]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(windows)]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

register_module!(pinyin, init);

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
  Ok(())
}

struct AsyncPinyinTask {
  style: PinyinStyle,
  input: String,
  should_to_multi: bool,
}

enum PinyinData {
  Multi(Vec<Vec<&'static str>>),
  Default(Vec<&'static str>),
}

impl Task for AsyncPinyinTask {
  type Output = PinyinData;
  type JsValue = JsObject;

  fn compute(&mut self) -> Result<Self::Output> {
    let mut index = 0;
    if self.should_to_multi {
      let pinyin_iter = (&self.input.as_str()).to_pinyin_multi();
      let (_, max) = pinyin_iter.size_hint();
      let mut result_arr: Vec<Vec<&'static str>> =
        Vec::with_capacity(max.unwrap_or(self.input.len()));
      for pinyin in pinyin_iter {
        if let Some(multi) = pinyin {
          let mut multi_arr = Vec::with_capacity(multi.count());
          let mut multi_index = 0;
          for pinyin in multi {
            multi_arr.insert(multi_index, get_pinyin(pinyin, self.style));
            multi_index += 1;
          }
          result_arr.insert(index, multi_arr);
          index += 1;
        }
      }
      Ok(PinyinData::Multi(result_arr))
    } else {
      let pinyin_iter = (&self.input.as_str()).to_pinyin();
      let (_, max) = pinyin_iter.size_hint();
      let mut result_arr = Vec::with_capacity(max.unwrap_or(self.input.len()));
      for pinyin in pinyin_iter {
        if let Some(pinyin) = pinyin {
          result_arr.insert(index, get_pinyin(pinyin, self.style));
          index += 1;
        }
      }
      Ok(PinyinData::Default(result_arr))
    }
  }

  fn resolve(self, env: Env, output: Self::Output) -> Result<Self::JsValue> {
    let js_value = match output {
      PinyinData::Default(arr) => {
        let mut output_arr = env.create_array_with_length(arr.len())?;

        for (index, item) in arr.into_iter().enumerate() {
          output_arr.set_element(index as u32, env.create_string(item)?)?;
        }

        output_arr
      }
      PinyinData::Multi(arr) => {
        let mut output_arr = env.create_array_with_length(arr.len())?;
        for (index, multi) in arr.into_iter().enumerate() {
          let mut multi_arr = env.create_array_with_length(multi.len())?;
          for (multi_index, item) in multi.into_iter().enumerate() {
            multi_arr.set_element(multi_index as u32, env.create_string(item)?)?;
          }
          output_arr.set_element(index as u32, multi_arr)?;
        }
        output_arr
      }
    };
    Ok(js_value)
  }
}

#[js_function(3)]
fn to_pinyin(ctx: CallContext) -> Result<JsObject> {
  let input_str = ctx.get::<JsString>(0)?.into_utf8()?;

  let should_to_multi = if ctx.length >= 2 {
    ctx.get::<JsBoolean>(1)?.get_value()?
  } else {
    false
  };

  let style = if ctx.length >= 3 {
    let style: u32 = ctx.get::<JsNumber>(2)?.try_into()?;
    style.try_into()?
  } else {
    PinyinStyle::Plain
  };

  let mut result_arr = ctx.env.create_array()?;
  let mut index = 0;

  if should_to_multi {
    let pinyin_iter = input_str.as_str()?.to_pinyin_multi();
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
      }
    }
  } else {
    let pinyin_iter = input_str.as_str()?.to_pinyin();
    for pinyin in pinyin_iter {
      if let Some(pinyin) = pinyin {
        result_arr.set_element(index, ctx.env.create_string(get_pinyin(pinyin, style))?)?;
        index += 1;
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

#[js_function(3)]
fn async_pinyin(ctx: CallContext) -> Result<JsObject> {
  let input = ctx.get::<JsString>(0)?.into_utf8()?.as_str()?.to_owned();
  let should_to_multi = if ctx.length >= 2 {
    ctx.get::<JsBoolean>(1)?.get_value()?
  } else {
    false
  };

  let style = if ctx.length >= 3 {
    let style: u32 = ctx.get::<JsNumber>(2)?.try_into()?;
    style.try_into()?
  } else {
    PinyinStyle::Plain
  };
  let task = AsyncPinyinTask {
    input,
    style,
    should_to_multi,
  };
  ctx.env.spawn(task).map(|v| v.promise_object())
}
