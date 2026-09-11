// Research-only NEON prefix scanner for jieba-rs's default character class.
// It accepts only common CJK or allowed ASCII; all other text uses Jieba's
// original classifier. Loads are bounded and input is already valid UTF-8.
use std::sync::atomic::{AtomicBool, Ordering};
static ENABLED: AtomicBool = AtomicBool::new(false);
pub fn set(enabled: bool) { ENABLED.store(enabled, Ordering::Relaxed); }
pub fn enabled() -> bool { ENABLED.load(Ordering::Relaxed) }

pub fn prefix(input: &[u8]) -> usize {
  #[cfg(target_arch = "aarch64")]
  unsafe {
    use std::arch::aarch64::*;
    if input.len() >= 16 && input[0].is_ascii() {
      let v = vld1q_u8(input.as_ptr());
      let between = |lo, hi| vandq_u8(vcgeq_u8(v, vdupq_n_u8(lo)), vcleq_u8(v, vdupq_n_u8(hi)));
      let mut mask = vorrq_u8(between(b'a', b'z'), vorrq_u8(between(b'A', b'Z'), between(b'0', b'9')));
      for c in [b'+', b'#', b'&', b'.', b'_', b'%', b'-'] {
        mask = vorrq_u8(mask, vceqq_u8(v, vdupq_n_u8(c)));
      }
      let bits = vget_lane_u64(vreinterpret_u64_u8(vshrn_n_u16(vreinterpretq_u16_u8(mask), 4)), 0);
      return ((!bits).trailing_zeros() as usize / 4).min(16);
    }
    if input.len() >= 48 {
      let bytes = vld3q_u8(input.as_ptr());
      let valid = vandq_u8(
        vceqq_u8(vandq_u8(bytes.0, vdupq_n_u8(0xf0)), vdupq_n_u8(0xe0)),
        vandq_u8(
          vceqq_u8(vandq_u8(bytes.1, vdupq_n_u8(0xc0)), vdupq_n_u8(0x80)),
          vceqq_u8(vandq_u8(bytes.2, vdupq_n_u8(0xc0)), vdupq_n_u8(0x80)),
        ),
      );
      if vminvq_u8(valid) == 255 {
        let a = vandq_u8(bytes.0, vdupq_n_u8(15));
        let b = vandq_u8(bytes.1, vdupq_n_u8(63));
        let c = vandq_u8(bytes.2, vdupq_n_u8(63));
        let lo = vorrq_u16(vshlq_n_u16(vmovl_u8(vget_low_u8(a)), 12),
          vorrq_u16(vshll_n_u8(vget_low_u8(b), 6), vmovl_u8(vget_low_u8(c))));
        let hi = vorrq_u16(vshlq_n_u16(vmovl_u8(vget_high_u8(a)), 12),
          vorrq_u16(vshll_n_u8(vget_high_u8(b), 6), vmovl_u8(vget_high_u8(c))));
        let mut count = 0;
        for codes in [lo, hi] {
          let mask = vandq_u16(vcgeq_u16(codes, vdupq_n_u16(0x4e00)), vcltq_u16(codes, vdupq_n_u16(0xa000)));
          let bits = vget_lane_u64(vreinterpret_u64_u8(vmovn_u16(mask)), 0);
          let n = ((!bits).trailing_zeros() as usize / 8).min(8);
          count += n * 3;
          if n != 8 { break; }
        }
        return count;
      }
    }
  }
  0
}
