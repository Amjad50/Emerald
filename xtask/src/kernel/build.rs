use std::path::PathBuf;

use crate::{
    args::Build,
    utils::{has_changed, run_cmd},
    GlobalMeta,
};

pub fn build_kernel(meta: &GlobalMeta, build: Build) -> anyhow::Result<PathBuf> {
    let kernel_path = super::kernel_path(meta);
    let elf_path = meta
        .target_path
        .join("x86-64-os")
        .join(meta.profile_path())
        .join("kernel");

    let cargo = std::env::var("CARGO")?;

    if has_changed(kernel_path.join("src/**/*"), &elf_path)?
        || has_changed(kernel_path.join("Cargo.toml"), &elf_path)?
    {
        let mut cmd = std::process::Command::new(cargo);

        cmd.current_dir(&kernel_path)
            .arg("build")
            .arg("--profile")
            .arg(meta.profile_name())
            .args(build.extra);

        run_cmd(cmd)?;
    } else {
        println!("[-] Kernel has not changed, skipping build");
    }

    Ok(elf_path)
}
