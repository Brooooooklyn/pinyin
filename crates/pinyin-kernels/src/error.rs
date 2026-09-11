use std::{collections::TryReserveError, fmt};

/// Failures encountered while preparing or transcoding text.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
  CapacityOverflow {
    input_len: usize,
  },
  Allocation {
    operation: &'static str,
    source: TryReserveError,
  },
  InvalidCursor {
    encoding: &'static str,
    offset: usize,
    input_len: usize,
  },
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::CapacityOverflow { input_len } => write!(f, "Cannot convert {input_len} UTF-16 units: the maximum UTF-8 output size exceeds usize::MAX"),
      Self::Allocation { operation, source } => write!(f, "Cannot allocate output for {operation}: {source}"),
      Self::InvalidCursor { encoding, offset, input_len } => write!(f, "Cannot decode {encoding} at offset {offset} in input of length {input_len}: expected a character boundary before the end of input"),
    }
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Allocation { source, .. } => Some(source),
      _ => None,
    }
  }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn utf8_capacity(input_len: usize) -> Result<usize> {
  input_len
    .checked_mul(3)
    .ok_or(Error::CapacityOverflow { input_len })
}

pub(crate) fn reserve<T>(
  output: &mut Vec<T>,
  additional: usize,
  operation: &'static str,
) -> Result<()> {
  output
    .try_reserve(additional)
    .map_err(|source| Error::Allocation { operation, source })
}

#[cfg(all(
  feature = "simd",
  any(
    target_arch = "aarch64",
    target_arch = "x86_64",
    all(target_arch = "wasm32", target_feature = "simd128")
  )
))]
pub(crate) fn next_utf8(input: &str, offset: usize) -> Result<char> {
  input
    .get(offset..)
    .and_then(|tail| tail.chars().next())
    .ok_or(Error::InvalidCursor {
      encoding: "UTF-8",
      offset,
      input_len: input.len(),
    })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[cfg(all(
    feature = "simd",
    any(
      target_arch = "aarch64",
      target_arch = "x86_64",
      all(target_arch = "wasm32", target_feature = "simd128")
    )
  ))]
  #[test]
  fn invalid_utf8_cursor_reports_encoding_and_offset() {
    for offset in [1, 3, 4] {
      let error = next_utf8("中", offset).unwrap_err();
      assert!(
        matches!(error, Error::InvalidCursor { encoding: "UTF-8", offset: actual, input_len: 3 } if actual == offset)
      );
      assert!(error.to_string().contains(&format!("offset {offset}")));
    }
  }

  #[test]
  fn oversized_output_is_an_error() {
    let err = utf8_capacity(usize::MAX).unwrap_err();
    assert!(matches!(
      err,
      Error::CapacityOverflow {
        input_len: usize::MAX
      }
    ));
    assert!(err.to_string().contains("UTF-16 units"));
    let err = reserve(&mut Vec::<u16>::new(), usize::MAX, "UTF-8 to UTF-16").unwrap_err();
    assert!(err.to_string().contains("UTF-8 to UTF-16"));
    assert!(std::error::Error::source(&err).is_some());
  }
}
