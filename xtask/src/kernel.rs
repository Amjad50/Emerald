use std::path::PathBuf;

use crate::GlobalMeta;

pub mod build;
pub mod check;
pub mod iso;
pub mod run;

fn iso_path(meta: &GlobalMeta) -> PathBuf {
    meta.target_path
        .join(meta.profile_path())
        .join("kernel.iso")
}

fn iso_src_folder(meta: &GlobalMeta) -> PathBuf {
    meta.target_path.join(meta.profile_path()).join("grub")
}

fn grub_src_path(meta: &GlobalMeta) -> PathBuf {
    kernel_path(meta).join("grub.cfg")
}

fn kernel_path(meta: &GlobalMeta) -> PathBuf {
    meta.root_path.join("kernel")
}
