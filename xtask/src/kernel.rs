use std::path::PathBuf;

use crate::GlobalMeta;

pub mod build;
pub mod check;
pub mod iso;
pub mod run;
pub mod test;

fn grub_src_path(meta: &GlobalMeta) -> PathBuf {
    kernel_path(meta).join("grub.cfg")
}

fn kernel_path(meta: &GlobalMeta) -> PathBuf {
    meta.root_path.join("kernel")
}
