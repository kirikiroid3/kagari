//! Little-endian reads and writes at `u64` offsets, shared by both formats. Every helper is
//! bounds-checked and returns `None` instead of panicking.

/// `len` bytes at `off`, or `None` when the range overflows or leaves `b`.
pub(crate) fn slice_at(b: &[u8], off: u64, len: u64) -> Option<&[u8]> {
    let start = usize::try_from(off).ok()?;
    let len = usize::try_from(len).ok()?;
    b.get(start..start.checked_add(len)?)
}

/// The mutable form of [`slice_at`].
pub(crate) fn slice_at_mut(b: &mut [u8], off: u64, len: u64) -> Option<&mut [u8]> {
    let start = usize::try_from(off).ok()?;
    let len = usize::try_from(len).ok()?;
    b.get_mut(start..start.checked_add(len)?)
}

fn array_at<const N: usize>(b: &[u8], off: u64) -> Option<[u8; N]> {
    slice_at(b, off, N as u64)?.try_into().ok()
}

fn put<const N: usize>(b: &mut [u8], off: u64, bytes: [u8; N]) -> Option<()> {
    slice_at_mut(b, off, N as u64)?.copy_from_slice(&bytes);
    Some(())
}

pub(crate) fn u16_at(b: &[u8], off: u64) -> Option<u16> {
    array_at(b, off).map(u16::from_le_bytes)
}

pub(crate) fn u32_at(b: &[u8], off: u64) -> Option<u32> {
    array_at(b, off).map(u32::from_le_bytes)
}

pub(crate) fn u64_at(b: &[u8], off: u64) -> Option<u64> {
    array_at(b, off).map(u64::from_le_bytes)
}

pub(crate) fn i128_at(b: &[u8], off: u64) -> Option<i128> {
    array_at(b, off).map(i128::from_le_bytes)
}

pub(crate) fn u128_at(b: &[u8], off: u64) -> Option<u128> {
    array_at(b, off).map(u128::from_le_bytes)
}

pub(crate) fn put_u16(b: &mut [u8], off: u64, v: u16) -> Option<()> {
    put(b, off, v.to_le_bytes())
}

pub(crate) fn put_u32(b: &mut [u8], off: u64, v: u32) -> Option<()> {
    put(b, off, v.to_le_bytes())
}

pub(crate) fn put_u64(b: &mut [u8], off: u64, v: u64) -> Option<()> {
    put(b, off, v.to_le_bytes())
}

pub(crate) fn put_i128(b: &mut [u8], off: u64, v: i128) -> Option<()> {
    put(b, off, v.to_le_bytes())
}

pub(crate) fn put_u128(b: &mut [u8], off: u64, v: u128) -> Option<()> {
    put(b, off, v.to_le_bytes())
}
