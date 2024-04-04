use std::{path::Path, process::Command};

use crate::{
    utils::{copy_files, has_changed, run_cmd},
    GlobalMeta,
};

use super::build::build_kernel;

fn iso_copy_grub_cfg(meta: &GlobalMeta) -> anyhow::Result<()> {
    copy_files(
        super::grub_src_path(meta),
        super::iso_src_folder(meta)
            .join("boot")
            .join("grub")
            .join("grub.cfg"),
    )
}

fn iso_copy_kernel(meta: &GlobalMeta) -> anyhow::Result<()> {
    let elf_path = build_kernel(meta)?;

    copy_files(
        elf_path,
        super::iso_src_folder(meta).join("boot").join("kernel"),
    )
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

pub fn build_iso(meta: &GlobalMeta) -> anyhow::Result<()> {
    let iso_src = super::iso_src_folder(meta);
    let iso_dst = super::iso_path(meta);

    std::fs::create_dir_all(iso_src.join("boot").join("grub"))?;

    iso_copy_kernel(meta)?;
    iso_copy_grub_cfg(meta)?;
    create_iso(&iso_src, &iso_dst)?;

    Ok(())
}
