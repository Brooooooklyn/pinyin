//! AVX-512 tier: 64-byte ASCII blocks and 21-triplet three-byte blocks using
//! VBMI byte permutes. Narrower inputs delegate from the dispatch wrapper.

use std::arch::x86_64::*;

/// Number of complete three-byte characters in a 64-byte block: bytes 0..63.
const TRIPLETS: usize = 21;
const TRIPLET_MASK: u64 = (1 << TRIPLETS) - 1;

/// vpermb indices picking every third byte; lanes past the 21 triplets are
/// never stored, so their index value does not matter.
const fn gather_idx(stream: usize) -> [u8; 64] {
  let mut idx = [63u8; 64];
  let mut t = 0usize;
  while t < TRIPLETS {
    idx[t] = (3 * t + stream) as u8;
    t += 1;
  }
  idx
}

/// Interleave index for output bytes `base..base + 64`: stream `g % 3` of
/// character `g / 3`. a and b bytes live in the first source register (a at
/// 0..32, b at 32..64), c bytes in the second (bit 6 selects it).
const fn interleave_idx(base: usize) -> [u8; 64] {
  let mut idx = [0u8; 64];
  let mut j = 0usize;
  while j < 64 {
    let g = base + j;
    let t = g / 3;
    idx[j] = match g % 3 {
      0 => t as u8,
      1 => (32 + t) as u8,
      _ => (64 + t) as u8,
    };
    j += 1;
  }
  idx
}

const GATHER: [[u8; 64]; 3] = [gather_idx(0), gather_idx(1), gather_idx(2)];
const INTERLEAVE_LO: [u8; 64] = interleave_idx(0);
const INTERLEAVE_HI: [u8; 64] = interleave_idx(64);

// Caller checks AVX-512F/BW/VL/VBMI support, supplies valid UTF-8 and room
// for at least min(input.len(), 64) units.
// SAFETY: every load is guarded by a 64-byte length check; the masked store
// writes exactly the 21 returned units.
#[target_feature(enable = "avx512f,avx512bw,avx512vl,avx512vbmi,avx2,ssse3")]
pub(super) unsafe fn utf8(input: &[u8], dst: *mut u16) -> (usize, usize) {
  if input.len() >= 64 {
    let v = _mm512_loadu_si512(input.as_ptr().cast());
    if _mm512_test_epi8_mask(v, _mm512_set1_epi8(-128)) == 0 {
      let lo = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(v));
      let hi = _mm512_cvtepu8_epi16(_mm512_extracti64x4_epi64(v, 1));
      _mm512_storeu_si512(dst.cast(), lo);
      _mm512_storeu_si512(dst.add(32).cast(), hi);
      return (64, 64);
    }
  }
  if input.len() >= 64 {
    let z = _mm512_loadu_si512(input.as_ptr().cast());
    let b0 = _mm512_permutexvar_epi8(_mm512_loadu_si512(GATHER[0].as_ptr().cast()), z);
    let b1 = _mm512_permutexvar_epi8(_mm512_loadu_si512(GATHER[1].as_ptr().cast()), z);
    let b2 = _mm512_permutexvar_epi8(_mm512_loadu_si512(GATHER[2].as_ptr().cast()), z);
    let splat = |x: u8| _mm512_set1_epi8(x as i8);
    let cont = |v: __m512i| _mm512_cmpeq_epi8_mask(_mm512_and_si512(v, splat(0xc0)), splat(0x80));
    let valid =
      _mm512_cmpeq_epi8_mask(_mm512_and_si512(b0, splat(0xf0)), splat(0xe0)) & cont(b1) & cont(b2);
    if valid & TRIPLET_MASK == TRIPLET_MASK {
      let w0 = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(b0));
      let w1 = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(b1));
      let w2 = _mm512_cvtepu8_epi16(_mm512_castsi512_si256(b2));
      let codes = _mm512_or_si512(
        _mm512_slli_epi16::<12>(_mm512_and_si512(w0, _mm512_set1_epi16(15))),
        _mm512_or_si512(
          _mm512_slli_epi16::<6>(_mm512_and_si512(w1, _mm512_set1_epi16(63))),
          _mm512_and_si512(w2, _mm512_set1_epi16(63)),
        ),
      );
      _mm512_mask_storeu_epi16(dst.cast(), TRIPLET_MASK as u32, codes);
      return (63, TRIPLETS);
    }
  }
  (0, 0)
}

// Caller checks AVX-512F/BW/VL/VBMI support and reserves three bytes per unit.
// SAFETY: ASCII loads 128 bytes for 64 units; stores write 64 or 96 bytes,
// both within three bytes per consumed unit.
#[target_feature(enable = "avx512f,avx512bw,avx512vl,avx512vbmi,avx2,ssse3")]
pub(super) unsafe fn utf16(input: &[u16], dst: *mut u8) -> (usize, usize) {
  if input.len() >= 64 {
    let a = _mm512_loadu_si512(input.as_ptr().cast());
    let b = _mm512_loadu_si512(input.as_ptr().add(32).cast());
    if _mm512_test_epi16_mask(_mm512_or_si512(a, b), _mm512_set1_epi16(-128)) == 0 {
      let pa = _mm512_cvtepi16_epi8(a);
      let pb = _mm512_cvtepi16_epi8(b);
      _mm512_storeu_si512(
        dst.cast(),
        _mm512_inserti64x4(_mm512_castsi256_si512(pa), pb, 1),
      );
      return (64, 64);
    }
  }
  if input.len() >= 32 {
    let v = _mm512_loadu_si512(input.as_ptr().cast());
    // BMP three-byte class: unit >= 0x800 and not a surrogate.
    let blocked = _mm512_cmplt_epu16_mask(v, _mm512_set1_epi16(0x800))
      | _mm512_cmplt_epu16_mask(
        _mm512_sub_epi16(v, _mm512_set1_epi16(0xd800u16 as i16)),
        _mm512_set1_epi16(0x800),
      );
    if blocked == 0 {
      let a = _mm512_or_si512(_mm512_srli_epi16::<12>(v), _mm512_set1_epi16(0xe0));
      let b = _mm512_or_si512(
        _mm512_and_si512(_mm512_srli_epi16::<6>(v), _mm512_set1_epi16(63)),
        _mm512_set1_epi16(0x80),
      );
      let c = _mm512_or_si512(
        _mm512_and_si512(v, _mm512_set1_epi16(63)),
        _mm512_set1_epi16(0x80),
      );
      let a8 = _mm512_cvtepi16_epi8(a);
      let b8 = _mm512_cvtepi16_epi8(b);
      let c8 = _mm512_cvtepi16_epi8(c);
      let zab = _mm512_inserti64x4(_mm512_castsi256_si512(a8), b8, 1);
      let zc = _mm512_castsi256_si512(c8);
      let lo = _mm512_permutex2var_epi8(zab, _mm512_loadu_si512(INTERLEAVE_LO.as_ptr().cast()), zc);
      let hi = _mm512_permutex2var_epi8(zab, _mm512_loadu_si512(INTERLEAVE_HI.as_ptr().cast()), zc);
      _mm512_storeu_si512(dst.cast(), lo);
      _mm256_storeu_si256(dst.add(64).cast(), _mm512_castsi512_si256(hi));
      return (32, 96);
    }
  }
  (0, 0)
}
