//! AVX2 tier: doubles the SSSE3 block widths for ASCII and three-byte text.
//! Narrower inputs delegate to the SSSE3 blocks from the dispatch wrapper.

use std::arch::x86_64::*;

// Caller checks AVX2 and SSSE3 support, supplies valid UTF-8 and room for at
// least min(input.len(), 32) units.
// SAFETY: ASCII loads 32 bytes, triplets load 16 bytes at 0 and 12; both are
// guarded by the length checks. Stores write only the returned units.
#[target_feature(enable = "avx2,ssse3")]
pub(super) unsafe fn utf8(input: &[u8], dst: *mut u16) -> (usize, usize) {
  if input.len() >= 32 {
    let v = _mm256_loadu_si256(input.as_ptr().cast());
    if _mm256_movemask_epi8(v) == 0 {
      let lo = _mm256_cvtepu8_epi16(_mm256_castsi256_si128(v));
      let hi = _mm256_cvtepu8_epi16(_mm256_extracti128_si256(v, 1));
      _mm256_storeu_si256(dst.cast(), lo);
      _mm256_storeu_si256(dst.add(16).cast(), hi);
      return (32, 32);
    }
  }
  if input.len() >= 24 {
    let (q0, m0) = quad(input.as_ptr());
    let (q1, m1) = quad(input.as_ptr().add(12));
    if m0 == 15 && m1 == 15 {
      // Each quad holds 4 u16 in its low 64 bits; pack them contiguously.
      _mm_storeu_si128(dst.cast(), _mm_unpacklo_epi64(q0, q1));
      return (24, 8);
    }
    if m0 == 15 {
      _mm_storel_epi64(dst.cast(), q0);
      return (12, 4);
    }
  }
  (0, 0)
}

/// Decode the first four three-byte characters of a 16-byte load; the mask
/// covers the four decoded lanes. Same construction as the SSSE3 block.
#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn quad(ptr: *const u8) -> (__m128i, i32) {
  let v = _mm_loadu_si128(ptr.cast());
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
  let zero = _mm_setzero_si128();
  let a = _mm_slli_epi16::<12>(_mm_unpacklo_epi8(_mm_and_si128(a, _mm_set1_epi8(15)), zero));
  let b = _mm_slli_epi16::<6>(_mm_unpacklo_epi8(_mm_and_si128(b, _mm_set1_epi8(63)), zero));
  let c = _mm_unpacklo_epi8(_mm_and_si128(c, _mm_set1_epi8(63)), zero);
  (
    _mm_or_si128(a, _mm_or_si128(b, c)),
    _mm_movemask_epi8(valid) & 15,
  )
}

// Caller checks AVX2 and SSSE3 support and reserves three bytes per unit.
// SAFETY: ASCII loads 64 bytes for 32 units; stores write 32 or 48 bytes,
// both within three bytes per consumed unit.
#[target_feature(enable = "avx2,ssse3")]
pub(super) unsafe fn utf16(input: &[u16], dst: *mut u8) -> (usize, usize) {
  if input.len() >= 32 {
    let a = _mm256_loadu_si256(input.as_ptr().cast());
    let b = _mm256_loadu_si256(input.as_ptr().add(16).cast());
    if _mm256_testz_si256(_mm256_or_si256(a, b), _mm256_set1_epi16(-128)) != 0 {
      let packed = _mm256_packus_epi16(a, b);
      _mm256_storeu_si256(dst.cast(), _mm256_permute4x64_epi64::<0b11011000>(packed));
      return (32, 32);
    }
  }
  if input.len() >= 16 {
    let v = _mm256_loadu_si256(input.as_ptr().cast());
    // BMP three-byte class: unit >= 0x800 and not a surrogate.
    let high = _mm256_and_si256(v, _mm256_set1_epi16(-2048));
    let blocked = _mm256_or_si256(
      _mm256_cmpeq_epi16(high, _mm256_setzero_si256()),
      _mm256_cmpeq_epi16(high, _mm256_set1_epi16(0xd800u16 as i16)),
    );
    if _mm256_testz_si256(blocked, _mm256_set1_epi16(-1)) != 0 {
      let a = _mm256_or_si256(_mm256_srli_epi16::<12>(v), _mm256_set1_epi16(0xe0));
      let b = _mm256_or_si256(
        _mm256_and_si256(_mm256_srli_epi16::<6>(v), _mm256_set1_epi16(63)),
        _mm256_set1_epi16(0x80),
      );
      let c = _mm256_or_si256(
        _mm256_and_si256(v, _mm256_set1_epi16(63)),
        _mm256_set1_epi16(0x80),
      );
      let a8 = _mm256_packus_epi16(a, a);
      let b8 = _mm256_packus_epi16(b, b);
      let c8 = _mm256_packus_epi16(c, c);
      // Per 128-bit lane this is the SSSE3 merge: lane 0 covers units 0..8,
      // lane 1 covers units 8..16, each lane emitting 24 contiguous bytes.
      let idx = |v: [i8; 16]| _mm256_broadcastsi128_si256(_mm_loadu_si128(v.as_ptr().cast()));
      let first = _mm256_or_si256(
        _mm256_shuffle_epi8(
          a8,
          idx([0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1, -1, 5]),
        ),
        _mm256_or_si256(
          _mm256_shuffle_epi8(
            b8,
            idx([-1, 0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1, -1]),
          ),
          _mm256_shuffle_epi8(
            c8,
            idx([-1, -1, 0, -1, -1, 1, -1, -1, 2, -1, -1, 3, -1, -1, 4, -1]),
          ),
        ),
      );
      let last = _mm256_or_si256(
        _mm256_shuffle_epi8(
          a8,
          idx([-1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1]),
        ),
        _mm256_or_si256(
          _mm256_shuffle_epi8(
            b8,
            idx([5, -1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1, -1]),
          ),
          _mm256_shuffle_epi8(
            c8,
            idx([-1, 5, -1, -1, 6, -1, -1, 7, -1, -1, -1, -1, -1, -1, -1, -1]),
          ),
        ),
      );
      _mm_storeu_si128(dst.cast(), _mm256_castsi256_si128(first));
      _mm_storel_epi64(dst.add(16).cast(), _mm256_castsi256_si128(last));
      _mm_storeu_si128(dst.add(24).cast(), _mm256_extracti128_si256(first, 1));
      _mm_storel_epi64(dst.add(40).cast(), _mm256_extracti128_si256(last, 1));
      return (16, 48);
    }
  }
  (0, 0)
}
