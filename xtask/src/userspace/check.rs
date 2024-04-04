use super::run_for_all_userspace_members;

pub fn check(meta: &crate::GlobalMeta) -> anyhow::Result<()> {
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("check");
    })
}

pub fn clippy(meta: &crate::GlobalMeta) -> anyhow::Result<()> {
    // TODO: we don't have clippy installed yet
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("clippy");
    })
}

pub fn fmt(meta: &crate::GlobalMeta) -> anyhow::Result<()> {
    run_for_all_userspace_members(meta, false, |cmd| {
        cmd.arg("fmt").arg("--check");
    })
}
