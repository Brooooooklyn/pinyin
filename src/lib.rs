#![deny(clippy::all)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod encoding;
mod error;
mod output;

use error::PinyinError;
use napi::{bindgen_prelude::*, ScopedTask};
use napi_derive::napi;
use pinyin_core::{Style, Token};
use std::{borrow::Cow, cell::RefCell, collections::HashMap, sync::LazyLock};

static JIEBA: LazyLock<std::result::Result<pinyin_core::jieba::Jieba, pinyin_core::jieba::Error>> =
  LazyLock::new(pinyin_core::jieba::Jieba::try_new);

fn jieba() -> Result<&'static pinyin_core::jieba::Jieba> {
  JIEBA
    .as_ref()
    .map_err(|source| PinyinError::Dictionary(source).into())
}

#[derive(Clone, Copy)]
enum Resolution {
  Character,
  Legacy,
  Phrase,
  Jieba,
}

impl Resolution {
  fn new(segment: Option<bool>, segmenter: Option<&str>, multi: bool) -> Result<Self> {
    let contextual = match segmenter {
      None => Self::Legacy,
      Some("phrase") => Self::Phrase,
      Some("jieba") => Self::Jieba,
      Some(other) => return Err(PinyinError::UnknownSegmenter(other.to_owned()).into()),
    };
    Ok(
      if segment.unwrap_or(false) && (!multi || matches!(contextual, Self::Legacy)) {
        contextual
      } else {
        Self::Character
      },
    )
  }

  fn tokens(self, input: &str) -> Result<ResolutionTokens<'_>> {
    if matches!(self, Self::Legacy) && !input.is_ascii() {
      return Ok(ResolutionTokens::Legacy(pinyin_core::jieba::legacy_tokens(
        input,
        jieba()?,
      )));
    }
    Ok(ResolutionTokens::Core(
      match self {
        Self::Legacy => pinyin_core::tokens(input, false),
        Self::Character => pinyin_core::tokens(input, false),
        Self::Phrase => pinyin_core::tokens(input, true),
        Self::Jieba if input.is_ascii() => pinyin_core::tokens(input, false),
        Self::Jieba => pinyin_core::jieba::tokens(input, jieba()?, false),
      }
      .map_err(PinyinError::from)?,
    ))
  }

  fn pinyin(self, input: &str, style: Style, separator: &str) -> Result<String> {
    Ok(match self {
      Self::Legacy => {
        let mut output = String::with_capacity(input.len().saturating_mul(2));
        for (i, token) in self.tokens(input)?.enumerate() {
          if i != 0 {
            output.push_str(separator);
          }
          output.push_str(token.text(input, style));
        }
        output
      }
      Self::Character => {
        pinyin_core::pinyin(input, style, false, separator).map_err(PinyinError::from)?
      }
      Self::Phrase => {
        pinyin_core::pinyin(input, style, true, separator).map_err(PinyinError::from)?
      }
      Self::Jieba if input.is_ascii() => input.to_owned(),
      Self::Jieba => pinyin_core::jieba::pinyin(input, style, jieba()?, false, separator)
        .map_err(PinyinError::from)?,
    })
  }

  fn pinyin_utf16(self, input: &str, style: Style, separator: &str) -> Result<Vec<u16>> {
    Ok(match self {
      Self::Legacy => {
        let mut output = Vec::with_capacity(input.len().saturating_mul(2));
        let separator: Vec<_> = separator.encode_utf16().collect();
        for (i, token) in self.tokens(input)?.enumerate() {
          if i != 0 {
            output.extend_from_slice(&separator);
          }
          if let Some(syllable) = token.syllable() {
            output.extend_from_slice(syllable.utf16(style));
          } else {
            encoding::append_utf16(token.text(input, style), &mut output)
              .map_err(PinyinError::from)?;
          }
        }
        output
      }
      Self::Character => {
        pinyin_core::utf16::pinyin(input, style, false, separator).map_err(PinyinError::from)?
      }
      Self::Phrase => {
        pinyin_core::utf16::pinyin(input, style, true, separator).map_err(PinyinError::from)?
      }
      Self::Jieba => pinyin_core::jieba::pinyin_utf16(input, style, jieba()?, false, separator)
        .map_err(PinyinError::from)?,
    })
  }
}

enum ResolutionTokens<'a> {
  Core(pinyin_core::Tokens<'a>),
  Legacy(pinyin_core::jieba::LegacyTokens<'a>),
}

impl Iterator for ResolutionTokens<'_> {
  type Item = Token;
  fn next(&mut self) -> Option<Token> {
    match self {
      Self::Core(tokens) => tokens.next(),
      Self::Legacy(tokens) => tokens.next(),
    }
  }
}

type EngineString = Either<Latin1String, Utf16String>;

// Copy JS's native representation, then transcode with consistent replacement
// semantics. This also avoids the WASI helper's malformed-surrogate UTF-8 bug.
type InputString = Utf16String;

fn string_input(input: &InputString) -> Result<String> {
  encoding::from_utf16_lossy(input).map_err(|source| PinyinError::from(source).into())
}

fn engine_string(value: String) -> Result<EngineString> {
  // ASCII already has V8's one-byte representation. Other output is encoded in
  // Rust before the boundary, avoiding V8's more expensive UTF-8 conversion.
  // The WASI runtime's Latin-1 helper stops at NUL even with an explicit length.
  // Its UTF-16 path preserves embedded NULs, as required by both public APIs.
  if value.is_ascii() && (!cfg!(target_family = "wasm") || !value.as_bytes().contains(&0)) {
    Ok(Either::A(value.into()))
  } else {
    let mut units = Vec::with_capacity(value.len());
    encoding::append_utf16(&value, &mut units).map_err(PinyinError::from)?;
    Ok(Either::B(units.into()))
  }
}

thread_local! {
  // FunctionRef owns only a persistent function reference. Its return type is
  // a marker; parse_array immediately scopes each fresh array to the calling Env.
  static JSON_PARSERS: RefCell<HashMap<usize, FunctionRef<EngineString, Array<'static>>>> = RefCell::new(HashMap::new());
}

#[napi(module_exports)]
pub fn initialize(_exports: Object, env: Env) -> Result<()> {
  let json: Object = env.get_global()?.get_named_property("JSON")?;
  let parse: Function<EngineString, Array<'static>> = json.get_named_property("parse")?;
  let reference = parse.create_ref()?;
  let key = env.raw() as usize;
  env.add_env_cleanup_hook(key, |key| {
    JSON_PARSERS.with(|parsers| parsers.borrow_mut().remove(&key));
  })?;
  JSON_PARSERS.with(|parsers| parsers.borrow_mut().insert(key, reference));
  Ok(())
}

fn parse_array<'env>(env: &'env Env, json: EngineString) -> Result<Array<'env>> {
  JSON_PARSERS.with(|parsers| {
    let parsers = parsers.borrow();
    let parse = parsers
      .get(&(env.raw() as usize))
      .ok_or(PinyinError::UninitializedEnvironment)?;
    parse.borrow_back(env)?.call(json)
  })
}

#[cfg(not(target_family = "wasm"))]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

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

impl From<PinyinStyle> for Style {
  fn from(style: PinyinStyle) -> Self {
    match style {
      PinyinStyle::Plain => Self::Plain,
      PinyinStyle::WithTone => Self::Tone,
      PinyinStyle::WithToneNum => Self::ToneNumber,
      PinyinStyle::WithToneNumEnd => Self::ToneNumberEnd,
      PinyinStyle::FirstLetter => Self::FirstLetter,
    }
  }
}

#[napi(object)]
#[derive(Default)]
pub struct PinyinConvertOptions {
  pub style: Option<PinyinStyle>,
  pub heteronym: Option<bool>,
  /// Use legacy Jieba segmentation by default, preserving per-character readings.
  pub segment: Option<bool>,
  /// Opt in to contextual phrase or Jieba readings when segment is true. With
  /// heteronym, these modes return all character readings. Jieba uses HMM=false.
  #[napi(ts_type = "'phrase' | 'jieba'")]
  pub segmenter: Option<String>,
}

#[napi(object)]
#[derive(Default)]
pub struct PinyinStringOptions {
  pub style: Option<PinyinStyle>,
  pub segment: Option<bool>,
  /// Opt in to contextual readings when segment is true; omission uses legacy
  /// Jieba segmentation and per-character readings. Jieba uses HMM=false.
  #[napi(ts_type = "'phrase' | 'jieba'")]
  pub segmenter: Option<String>,
  /// Separator between syllables and unchanged non-Han runs. Defaults to a space.
  #[napi(ts_type = "string")]
  pub separator: Option<Utf16String>,
}

fn utf8(input: &[u8]) -> Result<&str> {
  // Keep std-compatible offsets and early rejection, with SIMD on supported CPUs.
  simdutf8::compat::from_utf8(input).map_err(|source| PinyinError::InvalidUtf8(source).into())
}

fn input_str(input: Either<InputString, &[u8]>) -> Result<Cow<'_, str>> {
  match input {
    // Consume the temporary UTF-16 copy so it is freed before dictionary work
    // and output allocation. Byte input continues to borrow without copying.
    Either::A(input) => Ok(Cow::Owned(string_input(&input)?)),
    Either::B(input) => utf8(input).map(Cow::Borrowed),
  }
}

fn json_output(
  input: &str,
  tokens: impl Iterator<Item = Token>,
  style: Style,
  multi: bool,
) -> Result<EngineString> {
  if style == Style::Tone {
    return Ok(Either::B(
      output::json_utf16(input, tokens, style, multi)?.into(),
    ));
  }
  let mut joined = Vec::with_capacity(input.len().saturating_mul(2));
  joined.push(b'[');
  for (index, token) in tokens.enumerate() {
    if index != 0 {
      joined.push(b',');
    }
    if multi {
      joined.push(b'[');
    }
    if token.syllable().is_some() {
      joined.push(b'"');
      if multi {
        for (index, syllable) in token.readings().enumerate() {
          if index != 0 {
            joined.extend_from_slice(b"\",\"");
          }
          joined.extend_from_slice(syllable.text(style).as_bytes());
        }
      } else {
        joined.extend_from_slice(token.text(input, style).as_bytes());
      }
      joined.push(b'"');
    } else {
      output::json_utf8_text(token.text(input, style), &mut joined);
    }
    if multi {
      joined.push(b']');
    }
  }
  joined.push(b']');
  // SAFETY: dictionary syllables are valid UTF-8; escape_into preserves UTF-8,
  // and every delimiter written above is ASCII.
  engine_string(unsafe { String::from_utf8_unchecked(joined) })
}

fn prepare_output(
  input: &str,
  style: Style,
  resolution: Resolution,
  multi: bool,
) -> Result<PinyinOutput> {
  // Dispatch once so the hot array loop does not branch on resolver type for
  // every syllable. Each iterator gets its own monomorphized output writer.
  match resolution.tokens(input)? {
    ResolutionTokens::Core(tokens) => prepare_tokens(input, style, tokens, multi),
    ResolutionTokens::Legacy(tokens) => prepare_tokens(input, style, tokens, multi),
  }
}

fn prepare_tokens(
  input: &str,
  style: Style,
  mut tokens: impl Iterator<Item = Token>,
  multi: bool,
) -> Result<PinyinOutput> {
  // Keep only a bounded prefix while choosing the measured array crossover.
  // Large inputs stream into the encoded result without a full token vector.
  let threshold = if multi { 4 } else { 2 };
  let mut prefix = Vec::with_capacity(threshold);
  for _ in 0..threshold {
    match tokens.next() {
      Some(token) => prefix.push(token),
      None => return Ok(PinyinOutput::Direct(prefix)),
    }
  }
  Ok(PinyinOutput::Json(json_output(
    input,
    prefix.into_iter().chain(tokens),
    style,
    multi,
  )?))
}

fn to_js<'env>(
  env: &'env Env,
  input: &str,
  tokens: &[Token],
  style: Style,
  multi: bool,
) -> Result<Array<'env>> {
  let length = tokens
    .len()
    .try_into()
    .map_err(|_| PinyinError::TooManyTokens)?;
  let mut output = env.create_array(length)?;
  for (i, token) in tokens.iter().copied().enumerate() {
    if multi {
      let readings = token.readings();
      let mut inner = env.create_array(readings.len().max(1) as u32)?;
      if token.syllable().is_none() {
        inner.set(0, token.text(input, style))?;
      } else {
        for (j, syllable) in readings.enumerate() {
          if style == Style::Tone {
            inner.set(j as u32, env.create_string_utf16(syllable.utf16(style))?)?;
          } else {
            inner.set(j as u32, syllable.text(style))?;
          }
        }
      }
      output.set(i as u32, inner)?;
    } else if let Some(syllable) = token.syllable() {
      if style == Style::Tone {
        output.set(i as u32, env.create_string_utf16(syllable.utf16(style))?)?;
      } else {
        output.set(i as u32, syllable.text(style))?;
      }
    } else {
      output.set(i as u32, token.text(input, style))?;
    }
  }
  Ok(output)
}

#[napi(js_name = "pinyin", ts_return_type = "string[] | string[][]")]
pub fn to_pinyin<'env>(
  env: &'env Env,
  #[napi(ts_arg_type = "string | Uint8Array")] input: Either<InputString, &'env [u8]>,
  opt: Option<PinyinConvertOptions>,
) -> Result<Array<'env>> {
  let opt = opt.unwrap_or_default();
  let multi = opt.heteronym.unwrap_or(false);
  let text = input_str(input)?;
  let input = text.as_ref();
  let style = opt.style.unwrap_or(PinyinStyle::Plain).into();
  let resolution = Resolution::new(opt.segment, opt.segmenter.as_deref(), multi)?;
  match prepare_output(input, style, resolution, multi)? {
    PinyinOutput::Direct(tokens) => to_js(env, input, &tokens, style, multi),
    PinyinOutput::Json(json) => parse_array(env, json),
  }
}

/// Convert directly to one string, avoiding a JS allocation for each syllable.
#[napi(ts_return_type = "string")]
pub fn pinyin_string(
  #[napi(ts_arg_type = "string | Uint8Array")] input: Either<InputString, &[u8]>,
  opt: Option<PinyinStringOptions>,
) -> Result<EngineString> {
  let opt = opt.unwrap_or_default();
  let resolution = Resolution::new(opt.segment, opt.segmenter.as_deref(), false)?;
  let text = input_str(input)?;
  let input = text.as_ref();
  let style = opt.style.unwrap_or(PinyinStyle::Plain).into();
  let separator = opt.separator.map(|s| string_input(&s)).transpose()?;
  let separator = separator.as_deref().unwrap_or(" ");
  if style == Style::Tone {
    // This check already establishes the core's whole-ASCII shortcut. Avoid
    // scanning it again in the generic writer (significant for large buffers).
    if input.is_ascii() {
      return engine_string(input.to_owned());
    }
    return Ok(Either::B(
      resolution.pinyin_utf16(input, style, separator)?.into(),
    ));
  }
  engine_string(resolution.pinyin(input, style, separator)?)
}

pub enum PinyinOutput {
  Direct(Vec<Token>),
  // Both variants contain only Rust-owned memory, never worker-created JS handles.
  Json(EngineString),
}

pub struct AsyncPinyinTask {
  // Snapshot JS-owned buffers before queuing work, preserving the UTF-8/data
  // race fix even when the caller mutates or detaches the original Buffer.
  input: Either<String, Vec<u8>>,
  style: Style,
  resolution: Resolution,
  multi: bool,
}

impl AsyncPinyinTask {
  fn input(&self) -> Result<&str> {
    match &self.input {
      Either::A(input) => Ok(input),
      Either::B(input) => utf8(input),
    }
  }
}

impl<'task> ScopedTask<'task> for AsyncPinyinTask {
  type Output = PinyinOutput;
  type JsValue = Array<'task>;

  fn compute(&mut self) -> Result<Self::Output> {
    // Formatting and UTF-16 preparation belong on the worker too. Only the
    // unavoidable JS array allocation remains on the environment's thread.
    prepare_output(self.input()?, self.style, self.resolution, self.multi)
  }

  fn resolve(&mut self, env: &'task Env, output: Self::Output) -> Result<Self::JsValue> {
    match output {
      PinyinOutput::Direct(tokens) => to_js(env, self.input()?, &tokens, self.style, self.multi),
      PinyinOutput::Json(json) => parse_array(env, json),
    }
  }
}

#[napi(ts_return_type = "Promise<string[] | string[][]>")]
pub fn async_pinyin(
  #[napi(ts_arg_type = "string | Buffer")] input: Either<InputString, Buffer>,
  opt: Option<PinyinConvertOptions>,
  signal: Option<AbortSignal>,
) -> Result<AsyncTask<AsyncPinyinTask>> {
  let opt = opt.unwrap_or_default();
  let task = AsyncPinyinTask {
    input: match input {
      Either::A(input) => Either::A(string_input(&input)?),
      Either::B(input) => Either::B(input.as_ref().to_vec()),
    },
    style: opt.style.unwrap_or(PinyinStyle::Plain).into(),
    resolution: Resolution::new(
      opt.segment,
      opt.segmenter.as_deref(),
      opt.heteronym.unwrap_or(false),
    )?,
    multi: opt.heteronym.unwrap_or(false),
  };
  Ok(AsyncTask::with_optional_signal(task, signal))
}

#[napi]
pub fn compare(input_a: String, input_b: String) -> i32 {
  pinyin_core::compare(&input_a, &input_b) as i32
}
