use crate::{
    args::{Check, Clippy, Fmt},
    utils::run_cmd,
    GlobalMeta,
};

fn kernel_run_cargo(
    meta: &GlobalMeta,
    edit_cmd: impl FnOnce(&mut std::process::Command),
) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO")?;

    let mut cmd = std::process::Command::new(cargo);

    cmd.current_dir(super::kernel_path(meta));

    edit_cmd(&mut cmd);

    run_cmd(cmd)?;

    Ok(())
}

pub fn check(meta: &GlobalMeta, check: Check) -> anyhow::Result<()> {
    kernel_run_cargo(meta, |cmd| {
        cmd.arg("check").args(check.extra);
    })
}

pub fn clippy(meta: &GlobalMeta, clippy: Clippy) -> anyhow::Result<()> {
    kernel_run_cargo(meta, |cmd| {
        cmd.arg("clippy").args(clippy.extra);
    })
}

pub fn fmt(meta: &GlobalMeta, fmt: Fmt) -> anyhow::Result<()> {
    kernel_run_cargo(meta, |cmd| {
        cmd.arg("fmt").args(fmt.extra);
    })
}
