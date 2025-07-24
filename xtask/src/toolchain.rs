use std::path::PathBuf;

use crate::{
    args::Toolchain,
    utils::{copy_files, run_cmd},
    GlobalMeta,
};

// make sure we have the submodule checked out
fn check_rust_submodule(meta: &GlobalMeta) -> anyhow::Result<()> {
    let rust_path = meta.root_path.join("extern/rust");

    if !rust_path.exists() || !rust_path.join("library").exists() || !rust_path.join("src").exists()
    {
        anyhow::bail!(
            "Rust submodule is not checked out, run `git submodule update --init --recursive`"
        );
    }

    Ok(())
}

fn copy_config_toml(meta: &GlobalMeta) -> anyhow::Result<()> {
    copy_files(
        meta.root_path.join("extern/toolchain/config.toml"),
        meta.root_path.join("extern/rust/config.toml"),
    )
}

pub fn dist(meta: &GlobalMeta, opts: &Toolchain) -> anyhow::Result<()> {
    check_rust_submodule(meta)?;

    copy_config_toml(meta)?;

    let mut cmd = std::process::Command::new("python");

    cmd.current_dir(meta.root_path.join("extern/rust"))
        .env("GITHUB_ACTIONS", "false")
        .arg("x.py")
        .arg("dist")
        .arg("rustfmt")
        .arg("clippy")
        .arg("rustc")
        .arg("rust-std");

    run_cmd(cmd)?;

    let (folder, delete) = if let Some(out) = &opts.out {
        let path = PathBuf::from(out);

        if !path.exists() {
            std::fs::create_dir_all(&path)?;
        } else if !path.is_dir() {
            anyhow::bail!("Output path is not a directory: {:?}", path);
        }

        (path, false)
    } else if opts.install {
        (meta.target_path.join("toolchain"), true)
    } else {
        // nothing else to do, no need to copy
        return Ok(());
    };

    std::fs::create_dir_all(&folder)?;

    for file in meta.root_path.join("extern/rust/build/dist").read_dir()? {
        let file = file?;

        if file.file_type()?.is_file() {
            let path = file.path();
            let filename = path.file_name().unwrap().to_string_lossy();

            if !path.extension().map(|e| e == "xz").unwrap_or(false) {
                continue;
            }

            if filename.starts_with("rustfmt")
                || filename.starts_with("clippy")
                || filename.starts_with("rust-std")
                || (filename.starts_with("rustc-1") && !filename.contains("src"))
            {
                copy_files(path, folder.clone())?;
            }
        }
    }

    if opts.install {
        let mut cmd = std::process::Command::new("bash");
        // bash tools/install_toolchain_and_link.sh
        cmd.arg(meta.root_path.join("tools/install_toolchain_and_link.sh"))
            .arg(&folder);

        run_cmd(cmd)?;
    }

    if delete {
        std::fs::remove_dir_all(&folder)?;
    }

    Ok(())
}
