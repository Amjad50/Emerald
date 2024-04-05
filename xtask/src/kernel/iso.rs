use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    utils::{copy_files, has_changed, run_cmd},
    GlobalMeta,
};

use super::{build::build_kernel, test::build_test_kernel};

fn iso_copy_grub_cfg(meta: &GlobalMeta, iso_folder: &Path) -> anyhow::Result<()> {
    copy_files(
        super::grub_src_path(meta),
        iso_folder.join("boot").join("grub").join("grub.cfg"),
    )
}

fn iso_copy_kernel(elf_path: &Path, iso_folder: &Path) -> anyhow::Result<()> {
    copy_files(elf_path, iso_folder.join("boot").join("kernel"))
}

fn create_iso(input_folder: &Path, output_iso: &Path) -> anyhow::Result<()> {
    assert!(input_folder.is_dir(), "Input folder does not exist");
    assert!(
        output_iso.parent().unwrap().is_dir(),
        "Output folder does not exist"
    );

    if !has_changed(input_folder.join("**/*"), output_iso)? {
        println!("[-] ISO content has not changed, skipping creation");
        return Ok(());
    }

    let mut cmd = Command::new("grub-mkrescue");

    cmd.arg("-o").arg(output_iso).arg(input_folder);

    run_cmd(cmd)
}

fn common_finish_iso(
    meta: &GlobalMeta,
    iso_src: &Path,
    iso_dst: &Path,
    elf_path: &Path,
) -> anyhow::Result<()> {
    iso_copy_kernel(elf_path, iso_src)?;
    iso_copy_grub_cfg(meta, iso_src)?;
    create_iso(iso_src, iso_dst)?;

    Ok(())
}

pub fn build_normal_iso(meta: &GlobalMeta) -> anyhow::Result<PathBuf> {
    let iso_src = meta.target_path.join(meta.profile_path()).join("iso");
    let iso_dst = meta
        .target_path
        .join(meta.profile_path())
        .join("kernel.iso");

    std::fs::create_dir_all(iso_src.join("boot").join("grub"))?;

    let elf_path = build_kernel(meta, Default::default())?;

    common_finish_iso(meta, &iso_src, &iso_dst, &elf_path)?;

    Ok(iso_dst)
}

pub fn build_test_iso(meta: &GlobalMeta) -> anyhow::Result<PathBuf> {
    let iso_src = meta.target_path.join(meta.profile_path()).join("iso-test");
    let iso_dst = meta
        .target_path
        .join(meta.profile_path())
        .join("kernel-test.iso");

    std::fs::create_dir_all(iso_src.join("boot").join("grub"))?;
    let elf_path = build_test_kernel(meta)?;

    common_finish_iso(meta, &iso_src, &iso_dst, &elf_path)?;

    Ok(iso_dst)
}
