#[path = "../src/encoding.rs"]
mod encoding;

#[test]
fn escape_scan_matches_scalar_at_every_byte_offset_and_tail() {
  for offset in 0..17 {
    for len in 0..260 {
      let mut data = vec![b'a'; offset + len];
      assert_eq!(encoding::json_escape(&data[offset..]), None);
      for position in [0, len / 2, len.saturating_sub(1)] {
        if len == 0 {
          continue;
        }
        for byte in 0..=255 {
          data[offset + position] = byte;
          let expected = data[offset..]
            .iter()
            .position(|&b| b < 32 || b == b'"' || b == b'\\');
          assert_eq!(
            encoding::json_escape(&data[offset..]),
            expected,
            "{offset}/{len}/{position}/{byte}"
          );
          data[offset + position] = b'a';
        }
      }
    }
  }
}

#[test]
fn every_unicode_scalar_and_transcoder_tail_matches_std() {
  // Exercise real SIMD blocks, supplementary planes, alignment, and every tail.
  let scalars: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
  let expected: Vec<_> = scalars.encode_utf16().collect();
  let mut actual = vec![0xfeed];
  encoding::append_utf16(&scalars, &mut actual).unwrap();
  assert_eq!(&actual[1..], expected);
  for n in 0..260 {
    for prefix in 0..17 {
      let text = format!("{}é中🙂\0{}", "a".repeat(n), "𐀀".repeat(n % 7));
      let mut output = vec![0xbeef; prefix];
      encoding::append_utf16(&text, &mut output).unwrap();
      assert_eq!(&output[..prefix], vec![0xbeef; prefix]);
      assert_eq!(&output[prefix..], text.encode_utf16().collect::<Vec<_>>());
    }
  }
  #[cfg(all(
    not(target_family = "wasm"),
    any(target_arch = "aarch64", target_arch = "x86_64")
  ))]
  assert_eq!(encoding::from_utf16_lossy(&expected).unwrap(), scalars);
}

#[cfg(all(
  not(target_family = "wasm"),
  any(target_arch = "aarch64", target_arch = "x86_64")
))]
#[test]
fn malformed_utf16_and_unaligned_input_match_lossy_std() {
  for len in 0..260 {
    let valid: Vec<_> = "aé中🙂\0".repeat(len).encode_utf16().collect();
    for offset in 0..17 {
      let mut backing = vec![0; offset];
      backing.extend_from_slice(&valid);
      assert_eq!(
        encoding::from_utf16_lossy(&backing[offset..]).unwrap(),
        String::from_utf16_lossy(&valid)
      );
      for malformed in [
        vec![0xd800],
        vec![0xdc00],
        vec![0xd800, 0x61],
        vec![0xdc00, 0xd800],
      ] {
        for position in [0, valid.len() / 2, valid.len()] {
          let mut input = valid.clone();
          input.splice(position..position, malformed.iter().copied());
          assert_eq!(
            encoding::from_utf16_lossy(&input).unwrap(),
            String::from_utf16_lossy(&input)
          );
        }
      }
    }
  }
}
