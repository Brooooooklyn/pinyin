use std::arch::x86_64::*;

// Caller checks SSSE3 support, supplies valid UTF-8 and space for 16 units.
// SAFETY: input loads require 16 bytes; stores write only the returned units.
#[target_feature(enable = "ssse3")]
#[inline]
pub(super) unsafe fn utf8(input: &[u8], dst: *mut u16) -> (usize, usize) {
  if input.len() < 16 {
    return (0, 0);
  }
  let v = _mm_loadu_si128(input.as_ptr().cast());
  let zero = _mm_setzero_si128();
  if _mm_movemask_epi8(v) == 0 {
    _mm_storeu_si128(dst.cast(), _mm_unpacklo_epi8(v, zero));
    _mm_storeu_si128(dst.add(8).cast(), _mm_unpackhi_epi8(v, zero));
    return (16, 16);
  }
  let a = _mm_shuffle_epi8(
    v,
    _mm_setr_epi8(0, 3, 6, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
  );
  let b = _mm_shuffle_epi8(
    v,
    _mm_setr_epi8(1, 4, 7, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
  );
  let c = _mm_shuffle_epi8(
    v,
    _mm_setr_epi8(2, 5, 8, 11, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
  );
  let valid = _mm_and_si128(
    _mm_cmpeq_epi8(_mm_and_si128(a, _mm_set1_epi8(-16)), _mm_set1_epi8(-32)),
    _mm_and_si128(
      _mm_cmpeq_epi8(_mm_and_si128(b, _mm_set1_epi8(-64)), _mm_set1_epi8(-128)),
      _mm_cmpeq_epi8(_mm_and_si128(c, _mm_set1_epi8(-64)), _mm_set1_epi8(-128)),
    ),
  );
  if _mm_movemask_epi8(valid) & 15 == 15 {
    let a = _mm_slli_epi16::<12>(_mm_unpacklo_epi8(_mm_and_si128(a, _mm_set1_epi8(15)), zero));
    let b = _mm_slli_epi16::<6>(_mm_unpacklo_epi8(_mm_and_si128(b, _mm_set1_epi8(63)), zero));
    let c = _mm_unpacklo_epi8(_mm_and_si128(c, _mm_set1_epi8(63)), zero);
    _mm_storel_epi64(dst.cast(), _mm_or_si128(a, _mm_or_si128(b, c)));
    return (12, 4);
  }
  (0, 0)
}

// Caller checks SSSE3 support and reserves three bytes per input unit.
// SAFETY: loads are guarded for 32 or eight units; stores write 32, eight or 24 bytes.
#[target_feature(enable = "ssse3")]
#[inline]
pub(super) unsafe fn utf16(input: &[u16], dst: *mut u8) -> (usize, usize) {
  if input.len() < 8 {
    return (0, 0);
  }
  if input.len() >= 32 {
    let a = _mm_loadu_si128(input.as_ptr().cast());
    let b = _mm_loadu_si128(input.as_ptr().add(8).cast());
    let c = _mm_loadu_si128(input.as_ptr().add(16).cast());
    let d = _mm_loadu_si128(input.as_ptr().add(24).cast());
    let any = _mm_or_si128(_mm_or_si128(a, b), _mm_or_si128(c, d));
    if _mm_movemask_epi8(_mm_cmpeq_epi16(
      _mm_and_si128(any, _mm_set1_epi16(-128)),
      _mm_setzero_si128(),
    )) == 65535
    {
      _mm_storeu_si128(dst.cast(), _mm_packus_epi16(a, b));
      _mm_storeu_si128(dst.add(16).cast(), _mm_packus_epi16(c, d));
      return (32, 32);
    }
  }
  let v = _mm_loadu_si128(input.as_ptr().cast());
  let zero = _mm_setzero_si128();
  if _mm_movemask_epi8(_mm_cmpeq_epi16(
    _mm_and_si128(v, _mm_set1_epi16(-128)),
    zero,
  )) == 65535
  {
    _mm_storel_epi64(dst.cast(), _mm_packus_epi16(v, v));
    return (8, 8);
  }
  let below_bmp = _mm_cmpeq_epi16(_mm_and_si128(v, _mm_set1_epi16(-2048)), zero);
  let surrogate = _mm_cmpeq_epi16(
    _mm_and_si128(v, _mm_set1_epi16(-2048)),
    _mm_set1_epi16(0xd800u16 as i16),
  );
  if _mm_movemask_epi8(_mm_or_si128(below_bmp, surrogate)) == 0 {
    let a = _mm_or_si128(_mm_srli_epi16::<12>(v), _mm_set1_epi16(0xe0));
    let b = _mm_or_si128(
      _mm_and_si128(_mm_srli_epi16::<6>(v), _mm_set1_epi16(63)),
      _mm_set1_epi16(0x80),
    );
    let c = _mm_or_si128(_mm_and_si128(v, _mm_set1_epi16(63)), _mm_set1_epi16(0x80));
    let a = _mm_packus_epi16(a, a);
    let b = _mm_packus_epi16(b, b);
    let c = _mm_packus_epi16(c, c);
    let first = _mm_or_si128(
      _mm_shuffle_epi8(
        a,
        _mm_setr_epi8(0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1, -1, 5),
      ),
      _mm_or_si128(
        _mm_shuffle_epi8(
          b,
          _mm_setr_epi8(-1, 0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1, -1),
        ),
        _mm_shuffle_epi8(
          c,
          _mm_setr_epi8(-1, -1, 0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1),
        ),
      ),
    );
    let last = _mm_or_si128(
      _mm_shuffle_epi8(
        a,
        _mm_setr_epi8(-1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
      ),
      _mm_or_si128(
        _mm_shuffle_epi8(
          b,
          _mm_setr_epi8(5, -1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1),
        ),
        _mm_shuffle_epi8(
          c,
          _mm_setr_epi8(-1, 5, -1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1),
        ),
      ),
    );
    _mm_storeu_si128(dst.cast(), first);
    _mm_storel_epi64(dst.add(16).cast(), last);
    return (8, 24);
  }
  (0, 0)
}
