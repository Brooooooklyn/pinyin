use std::arch::aarch64::*;

// Caller supplies valid UTF-8 and room for at least min(input.len(), 16) units.
// SAFETY: NEON is baseline on aarch64; each load is guarded by its exact size.
#[inline]
pub(super) unsafe fn utf8(input: &[u8], dst: *mut u16) -> (usize, usize) {
  if input.len() >= 16 {
    let v = vld1q_u8(input.as_ptr());
    if vmaxvq_u8(v) < 128 {
      vst1q_u16(dst, vmovl_u8(vget_low_u8(v)));
      vst1q_u16(dst.add(8), vmovl_high_u8(v));
      return (16, 16);
    }
  }
  if input.len() >= 24 {
    let uint8x8x3_t(a, b, c) = vld3_u8(input.as_ptr());
    let valid = vand_u8(
      vceq_u8(vand_u8(a, vdup_n_u8(0xf0)), vdup_n_u8(0xe0)),
      vand_u8(
        vceq_u8(vand_u8(b, vdup_n_u8(0xc0)), vdup_n_u8(0x80)),
        vceq_u8(vand_u8(c, vdup_n_u8(0xc0)), vdup_n_u8(0x80)),
      ),
    );
    if vminv_u8(valid) == 255 {
      let value = vorrq_u16(
        vshlq_n_u16::<12>(vmovl_u8(vand_u8(a, vdup_n_u8(15)))),
        vorrq_u16(
          vshlq_n_u16::<6>(vmovl_u8(vand_u8(b, vdup_n_u8(63)))),
          vmovl_u8(vand_u8(c, vdup_n_u8(63))),
        ),
      );
      vst1q_u16(dst, value);
      return (24, 8);
    }
  }
  (0, 0)
}

// Caller supplies output capacity for three bytes per input unit.
// SAFETY: loads are guarded for 32 or eight units; stores write 32, eight or 24 bytes.
#[inline]
pub(super) unsafe fn utf16(input: &[u16], dst: *mut u8) -> (usize, usize) {
  if input.len() < 8 {
    return (0, 0);
  }
  if input.len() >= 32 {
    let a = vld1q_u16(input.as_ptr());
    let b = vld1q_u16(input.as_ptr().add(8));
    let c = vld1q_u16(input.as_ptr().add(16));
    let d = vld1q_u16(input.as_ptr().add(24));
    if vmaxvq_u16(vorrq_u16(vorrq_u16(a, b), vorrq_u16(c, d))) < 128 {
      vst1q_u8(dst, vcombine_u8(vmovn_u16(a), vmovn_u16(b)));
      vst1q_u8(dst.add(16), vcombine_u8(vmovn_u16(c), vmovn_u16(d)));
      return (32, 32);
    }
  }
  let v = vld1q_u16(input.as_ptr());
  if vmaxvq_u16(v) < 128 {
    vst1_u8(dst, vmovn_u16(v));
    return (8, 8);
  }
  let bmp = vandq_u16(
    vcgeq_u16(v, vdupq_n_u16(0x800)),
    vorrq_u16(
      vcltq_u16(v, vdupq_n_u16(0xd800)),
      vcgeq_u16(v, vdupq_n_u16(0xe000)),
    ),
  );
  if vminvq_u16(bmp) == u16::MAX {
    let a = vorr_u8(vmovn_u16(vshrq_n_u16::<12>(v)), vdup_n_u8(0xe0));
    let b = vorr_u8(
      vand_u8(vmovn_u16(vshrq_n_u16::<6>(v)), vdup_n_u8(63)),
      vdup_n_u8(0x80),
    );
    let c = vorr_u8(vand_u8(vmovn_u16(v), vdup_n_u8(63)), vdup_n_u8(0x80));
    vst3_u8(dst, uint8x8x3_t(a, b, c));
    return (8, 24);
  }
  (0, 0)
}
