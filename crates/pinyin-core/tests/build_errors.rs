#[path = "../build_dictionary.rs"]
mod build_dictionary;
#[allow(dead_code)] // Cargo-only filesystem errors are not used by the pure generator.
#[path = "../build_error.rs"]
mod build_error;

#[test]
fn invalid_character_data_reports_file_line_and_reason() {
  for (entry, reason) in [
    ("U+4E2D zhōng", "expected U+<hex>"),
    ("4E2D: zhōng", "must start with U+"),
    ("U+XYZ: zhōng", "must be hexadecimal"),
    ("U+D800: zhōng", "not a Unicode scalar"),
    ("U+110000: zhōng", "not a Unicode scalar"),
    ("U+4E2D: ", "nonempty readings"),
    ("U+4E2D: zhōng,", "nonempty readings"),
    ("U+4E2D: zhōōng", "multiple tones"),
    ("U+4E2D: z\"hong", "JSON escaping"),
  ] {
    let error =
      build_dictionary::generate(&format!("# comment\n{entry}"), "", false, false).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("data/characters.txt:2:"), "{message}");
    assert!(message.contains(reason), "{message}");
  }
}

#[test]
fn invalid_phrase_data_reports_file_line_and_reason() {
  for (entry, reason) in [
    ("中 zhōng", "separated by a tab"),
    ("\tzhōng", "must not be empty"),
    ("中中\tzhōng", "does not match"),
    ("国\tguó", "has no dictionary entry"),
    ("中\tzhōng\n中\tzhōng", "duplicate phrase"),
  ] {
    let error = build_dictionary::generate("U+4E2D: zhōng", entry, true, true).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("data/phrases.tsv:"), "{message}");
    assert!(message.contains(reason), "{message}");
  }
}

#[test]
fn index_overflow_and_ascii_mapping_are_errors() {
  let error = build_error::index::<u16>(65536, "syllable").unwrap_err();
  assert!(error
    .to_string()
    .contains("syllable index 65536 exceeds u16 capacity"));
  let error = build_dictionary::generate("U+0041: a", "", false, false).unwrap_err();
  assert!(error.to_string().contains("requires unmapped ASCII"));
}
