use std::fmt;

/// Application errors, translated to N-API errors only at the binding boundary.
#[derive(Debug)]
pub enum PinyinError {
  Encoding(napi_pinyin_kernels::Error),
  Conversion(pinyin_core::Error),
  Dictionary(&'static pinyin_core::jieba::Error),
  InvalidUtf8(simdutf8::compat::Utf8Error),
  UnknownSegmenter(String),
  UninitializedEnvironment,
  TooManyTokens,
}

impl fmt::Display for PinyinError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Encoding(source) => write!(f, "Pinyin text encoding failed: {source}"),
      Self::Conversion(source) => source.fmt(f),
      Self::Dictionary(source) => {
        write!(f, "Cannot initialize Jieba's embedded dictionary: {source}")
      }
      Self::InvalidUtf8(source) => write!(f, "Input buffer must contain valid UTF-8: {source}"),
      Self::UnknownSegmenter(name) => {
        write!(f, "Unknown segmenter: {name}; expected phrase or jieba")
      }
      Self::UninitializedEnvironment => f.write_str("Pinyin environment is not initialized"),
      Self::TooManyTokens => f.write_str("Too many output tokens"),
    }
  }
}

impl std::error::Error for PinyinError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Encoding(source) => Some(source),
      Self::Conversion(source) => Some(source),
      Self::Dictionary(source) => Some(*source),
      Self::InvalidUtf8(source) => Some(source),
      _ => None,
    }
  }
}

impl From<napi_pinyin_kernels::Error> for PinyinError {
  fn from(source: napi_pinyin_kernels::Error) -> Self {
    Self::Encoding(source)
  }
}

impl From<pinyin_core::Error> for PinyinError {
  fn from(source: pinyin_core::Error) -> Self {
    Self::Conversion(source)
  }
}

impl From<PinyinError> for napi::Error {
  fn from(error: PinyinError) -> Self {
    let status = match error {
      PinyinError::InvalidUtf8(_)
      | PinyinError::UnknownSegmenter(_)
      | PinyinError::TooManyTokens => napi::Status::InvalidArg,
      _ => napi::Status::GenericFailure,
    };
    Self::new(status, error.to_string())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validation_errors_preserve_javascript_codes_and_messages() {
    let source = simdutf8::compat::from_utf8(&[b'a', 0xff]).unwrap_err();
    let error = PinyinError::InvalidUtf8(source);
    assert!(std::error::Error::source(&error).is_some());
    let js: napi::Error = error.into();
    assert_eq!(js.status, napi::Status::InvalidArg);
    assert_eq!(
      js.reason,
      "Input buffer must contain valid UTF-8: invalid utf-8 sequence of 1 bytes from index 1"
    );
    let js: napi::Error = PinyinError::UnknownSegmenter("invalid".into()).into();
    assert_eq!(js.status, napi::Status::InvalidArg);
    assert_eq!(
      js.reason,
      "Unknown segmenter: invalid; expected phrase or jieba"
    );
  }

  #[test]
  fn initialization_and_encoding_errors_reach_the_js_boundary() {
    static DICTIONARY_ERROR: std::sync::LazyLock<pinyin_core::jieba::Error> =
      std::sync::LazyLock::new(|| {
        pinyin_core::jieba::Error::InvalidDictEntry("bad frequency on line 7".into())
      });
    let error = PinyinError::Dictionary(&DICTIONARY_ERROR);
    assert!(std::error::Error::source(&error).is_some());
    let js: napi::Error = error.into();
    assert_eq!(js.status, napi::Status::GenericFailure);
    assert!(js.reason.contains("Jieba's embedded dictionary"));
    assert!(js.reason.contains("bad frequency on line 7"));
    let js: napi::Error = PinyinError::Encoding(napi_pinyin_kernels::Error::CapacityOverflow {
      input_len: usize::MAX,
    })
    .into();
    assert_eq!(js.status, napi::Status::GenericFailure);
    assert!(js.reason.contains("UTF-16 units"));
  }
}
