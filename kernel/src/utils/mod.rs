pub mod vcell;

#[repr(C)]
pub struct Pad<const N: usize> {
    _pad: [u8; N],
}
