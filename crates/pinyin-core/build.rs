#![deny(clippy::unwrap_used, clippy::expect_used)]
mod build_dictionary;
mod build_error;

fn main() -> build_error::Result<()> {
  for path in [
    "build.rs",
    "build_dictionary.rs",
    "build_error.rs",
    "data/characters.txt",
    "data/phrases.tsv",
  ] {
    println!("cargo:rerun-if-changed={path}");
  }
  let output = build_dictionary::generate(
    include_str!("data/characters.txt"),
    include_str!("data/phrases.tsv"),
    std::env::var_os("CARGO_FEATURE_UTF16").is_some(),
    std::env::var_os("CARGO_FEATURE_SIMD").is_some(),
  )?;
  let path = std::path::PathBuf::from(
    std::env::var_os("OUT_DIR").ok_or(build_error::BuildError::MissingOutputDirectory)?,
  )
  .join("dictionary.rs");
  std::fs::write(&path, output).map_err(|source| build_error::BuildError::Write { path, source })
}
