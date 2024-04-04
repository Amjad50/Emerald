pub mod check;

use cargo_metadata::Package;

use crate::{
    args::Build,
    utils::{copy_files, has_changed, run_cmd},
    GlobalMeta,
};

const TARGET: &str = "x86_64-unknown-emerald";

fn toolchain_path(meta: &GlobalMeta) -> std::path::PathBuf {
    meta.root_path
        .join("extern/toolchain")
        .canonicalize()
        .unwrap()
}

fn check_toolchain_installed(meta: &GlobalMeta) -> anyhow::Result<()> {
    let toolchain_path = toolchain_path(meta);

    if !toolchain_path.exists()
        || !toolchain_path.join("bin").exists()
        || !toolchain_path.join("lib").exists()
        || !toolchain_path.join("bin").join("rustc").exists()
    {
        anyhow::bail!(
            "Toolchain not found at {:?}, run `cargo xtask toolchain --install` to build and install it\nOr install it from a prebuilt package with `bash tools/install_toolchain_and_link.sh <toolchain-dir-or-zip>`",
            toolchain_path
        );
    }

    Ok(())
}

fn userspace_output_path(meta: &GlobalMeta, name: &str) -> std::path::PathBuf {
    meta.target_path
        .join(TARGET)
        .join(meta.profile_path())
        .join(name.replace('-', "_"))
}

fn userspace_packages(meta: &GlobalMeta) -> impl Iterator<Item = &cargo_metadata::Package> {
    meta.meta
        .workspace_packages()
        .into_iter()
        .filter(|package| {
            package
                .manifest_path
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .ends_with("userspace")
        })
}

fn has_changed_for_package(meta: &GlobalMeta, package: &Package) -> anyhow::Result<bool> {
    let package_path = package.manifest_path.parent().unwrap();

    for target in package
        .targets
        .iter()
        .filter(|p| p.kind.contains(&"bin".to_string()))
    {
        let elf_path = userspace_output_path(meta, &target.name);

        if has_changed(package_path.join("src/**/*"), &elf_path)?
            || has_changed(package_path.join("Cargo.toml"), &elf_path)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn run_for_all_userspace_members(
    meta: &GlobalMeta,
    cargo_cmd: &str,
    edit_cmd: impl Fn(&mut std::process::Command),
) -> anyhow::Result<()> {
    let userspace_packages = userspace_packages(meta);

    let toolchain_prefix = "+".to_string() + toolchain_path(meta).to_str().unwrap();

    let mut packages_to_run = Vec::new();

    for package in userspace_packages {
        let should_run = if cargo_cmd == "build" {
            has_changed_for_package(meta, package)?
        } else {
            // run always
            true
        };

        if !should_run {
            println!(
                "[-] Package {:?} has not changed, skipping run",
                package.name
            );
            continue;
        }

        packages_to_run.push(&package.name)
    }

    let mut cmd = std::process::Command::new("cargo");

    cmd.current_dir(meta.root_path.join("userspace"))
        .arg(&toolchain_prefix)
        .arg(cargo_cmd);

    for package in packages_to_run {
        cmd.arg("-p").arg(package);
    }

    edit_cmd(&mut cmd);

    run_cmd(cmd)?;

    Ok(())
}

pub fn copy_to_filesystem(meta: &GlobalMeta) -> anyhow::Result<()> {
    let userspace_packages = userspace_packages(meta);

    for package in userspace_packages {
        for target in package
            .targets
            .iter()
            .filter(|p| p.kind.contains(&"bin".to_string()))
        {
            copy_files(
                userspace_output_path(meta, &target.name),
                meta.filesystem_path.join(target.name.as_str()),
            )?;
        }
    }

    Ok(())
}

fn build(meta: &GlobalMeta, build: Build) -> anyhow::Result<()> {
    run_for_all_userspace_members(meta, "build", |cmd| {
        cmd.arg("--profile")
            .arg(meta.profile_name())
            .arg("--target")
            .arg(TARGET)
            .args(&build.extra);
    })
}

pub fn build_programs(meta: &GlobalMeta, cmd: Build) -> anyhow::Result<()> {
    check_toolchain_installed(meta)?;
    build(meta, cmd)?;
    copy_to_filesystem(meta)?;

    Ok(())
}
