use std::{fmt, io, path::PathBuf};

#[derive(Debug)]
pub enum BuildError {
  InvalidData(String),
  Format(fmt::Error),
  MissingOutputDirectory,
  Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for BuildError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::InvalidData(message) => write!(f, "Cannot generate pinyin dictionary: {message}"),
      Self::Format(source) => write!(f, "Cannot format generated pinyin dictionary: {source}"),
      Self::MissingOutputDirectory => {
        f.write_str("Cannot write pinyin dictionary: Cargo did not set OUT_DIR")
      }
      Self::Write { path, source } => write!(
        f,
        "Cannot write pinyin dictionary to {}: {source}",
        path.display()
      ),
    }
  }
}

impl std::error::Error for BuildError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Format(source) => Some(source),
      Self::Write { source, .. } => Some(source),
      _ => None,
    }
  }
}

impl From<fmt::Error> for BuildError {
  fn from(source: fmt::Error) -> Self {
    Self::Format(source)
  }
}

pub type Result<T> = std::result::Result<T, BuildError>;

pub fn require(condition: bool, message: impl Into<String>) -> Result<()> {
  if condition {
    Ok(())
  } else {
    Err(BuildError::InvalidData(message.into()))
  }
}

pub fn index<T: TryFrom<usize>>(value: usize, name: &str) -> Result<T> {
  T::try_from(value).map_err(|_| {
    BuildError::InvalidData(format!(
      "{name} index {value} exceeds {} capacity",
      std::any::type_name::<T>()
    ))
  })
}
