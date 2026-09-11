use napi_pinyin_kernels as encoding;

#[test]
fn ascii_prefix_is_bounded_at_all_alignments() {
  for offset in 0..32 {
    for len in 0..260 {
      let mut input = vec![0x7f; offset + len];
      assert_eq!(encoding::ascii_prefix(&input[offset..]), len);
      for i in 0..len {
        for byte in [128, 192, 224, 240, 255] {
          input[offset + i] = byte;
          assert_eq!(encoding::ascii_prefix(&input[offset..]), i);
        }
        input[offset + i] = 0x7f;
      }
    }
  }
}

#[test]
fn decoded_vectors_cover_unicode_and_adaptive_boundaries() {
  let all: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
  assert_eq!(
    encoding::decode(&all).unwrap(),
    all.chars().collect::<Vec<_>>()
  );
  let wrapped = format!("{}{}{}", "中".repeat(64), all, "国".repeat(64));
  assert_eq!(
    encoding::decode(&wrapped).unwrap(),
    wrapped.chars().collect::<Vec<_>>()
  );
  for n in [0, 1, 31, 32, 63, 64, 170, 171, 512, 1000] {
    for middle in ["", "ASCII", "\0é🙂", "𠀀", "\u{d7ff}\u{e000}", "a中"] {
      let text = format!("{}{}{}", "中".repeat(n), middle.repeat(n), "国".repeat(n));
      assert_eq!(
        encoding::decode(&text).unwrap(),
        text.chars().collect::<Vec<_>>()
      );
    }
  }
}

#[test]
fn packed_search_selects_first_lane_and_preserves_high_bits() {
  for labels in [
    [1, 2, 3, 4],
    [0, 0, 0, 0],
    [0xffffffff, 0x80000000, 0x10ffff, 0],
    [3, 1, 3, 1],
  ] {
    for code in labels.into_iter().chain([5, 0x7fffffff, 0x10fffe]) {
      assert_eq!(
        encoding::find4(&labels, code),
        labels.iter().position(|&x| x == code)
      );
    }
  }
}

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
  assert_eq!(encoding::from_utf16_lossy(&expected).unwrap(), scalars);
  // Force adaptive SIMD paths while exercising every scalar in the middle.
  let wrapped = format!("{}{}{}", "中".repeat(64), scalars, "国".repeat(64));
  let expected: Vec<_> = wrapped.encode_utf16().collect();
  let mut output = vec![0xbeef];
  encoding::append_utf16(&wrapped, &mut output).unwrap();
  assert_eq!(&output[1..], expected);
  assert_eq!(encoding::from_utf16_lossy(&expected).unwrap(), wrapped);
  for n in [31, 32, 127, 128, 169, 170, 171, 511, 512, 513] {
    for block in ["a", "中", "a中", "é🙂"] {
      let text = format!("{}{}{}", "中".repeat(64), block.repeat(n), "国".repeat(64));
      let mut units: Vec<_> = text.encode_utf16().collect();
      let mut output = Vec::new();
      encoding::append_utf16(&text, &mut output).unwrap();
      assert_eq!(output, units);
      units.insert(units.len() / 2, 0xd800);
      assert_eq!(
        encoding::from_utf16_lossy(&units).unwrap(),
        String::from_utf16_lossy(&units)
      );
    }
  }
}

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
