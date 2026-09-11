use std::{collections::TryReserveError, fmt};

/// A failure while preparing text for pinyin conversion.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
  Allocation(TryReserveError),
  #[cfg(feature = "simd")]
  Encoding(napi_pinyin_kernels::Error),
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Allocation(source) => write!(
        f,
        "Cannot allocate characters for pinyin phrase resolution: {source}"
      ),
      #[cfg(feature = "simd")]
      Self::Encoding(source) => write!(f, "Cannot prepare text for pinyin conversion: {source}"),
    }
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Allocation(source) => Some(source),
      #[cfg(feature = "simd")]
      Self::Encoding(source) => Some(source),
    }
  }
}

#[cfg(feature = "simd")]
impl From<napi_pinyin_kernels::Error> for Error {
  fn from(source: napi_pinyin_kernels::Error) -> Self {
    Self::Encoding(source)
  }
}

pub type Result<T> = std::result::Result<T, Error>;
