use crate::{
    args::{Check, Clippy, Fmt},
    GlobalMeta,
};

use super::{check_toolchain_installed, run_for_all_userspace_members};

pub fn check(meta: &GlobalMeta, check: Check) -> anyhow::Result<()> {
    check_toolchain_installed(meta)?;
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("check").args(&check.extra);
    })
}

pub fn clippy(meta: &GlobalMeta, clippy: Clippy) -> anyhow::Result<()> {
    check_toolchain_installed(meta)?;
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("clippy").args(&clippy.extra);
    })
}

pub fn fmt(meta: &GlobalMeta, fmt: Fmt) -> anyhow::Result<()> {
    check_toolchain_installed(meta)?;
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("fmt").args(&fmt.extra);
    })
}
