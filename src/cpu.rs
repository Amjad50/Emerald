#[macro_export]
macro_rules! pause {
    () => {
        unsafe {
            core::arch::asm!("pause");
        }
    };
}
pub use pause;

// TODO: implement cpu_id
pub fn cpu_id() -> u32 {
    0
}
