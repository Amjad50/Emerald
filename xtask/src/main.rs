mod args;
mod kernel;
mod toolchain;
mod userspace;
mod utils;

use std::path::PathBuf;

use utils::NoDebug;

use crate::args::{Args, Command, RustMiscCmd};

#[derive(Debug)]
struct GlobalMeta {
    release: bool,
    target_path: PathBuf,
    root_path: PathBuf,
    filesystem_path: PathBuf,
    meta: NoDebug<cargo_metadata::Metadata>,
}

impl GlobalMeta {
    pub fn load(release: bool) -> anyhow::Result<Self> {
        let metadata = cargo_metadata::MetadataCommand::new().exec().unwrap();

        let target_path = metadata.target_directory.clone().into_std_path_buf();
        let root_path = metadata.workspace_root.clone().into_std_path_buf();

        Ok(Self {
            release,
            target_path,
            filesystem_path: root_path.join("filesystem"),
            root_path,
            meta: NoDebug(metadata),
        })
    }

    pub fn profile_path(&self) -> &'static str {
        if self.release {
            "release"
        } else {
            "debug"
        }
    }

    pub fn profile_name(&self) -> &'static str {
        if self.release {
            "release"
        } else {
            "dev"
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    let meta = GlobalMeta::load(args.release)?;

    match args.cmd {
        Command::Run(run) => {
            kernel::iso::build_iso(&meta)?;
            userspace::build_programs(&meta, Default::default())?;
            let result = kernel::run::RunConfig::new()
                .with_serial(true)
                .with_gdb(run.gdb)
                .run(&meta, &run.extra)?;

            std::process::exit(result);
        }
        Command::BuildIso(_) => {
            kernel::iso::build_iso(&meta)?;
        }
        Command::Kernel(cmd) => match cmd.cmd {
            RustMiscCmd::Build(build) => kernel::build::build_kernel(&meta, build).map(|_| ())?,
            RustMiscCmd::Check(check) => kernel::check::check(&meta, check)?,
            RustMiscCmd::Clippy(clippy) => kernel::check::clippy(&meta, clippy)?,
            RustMiscCmd::Fmt(fmt) => kernel::check::fmt(&meta, fmt)?,
        },
        Command::Userspace(cmd) => match cmd.cmd {
            RustMiscCmd::Build(build) => userspace::build_programs(&meta, build).map(|_| ())?,
            RustMiscCmd::Check(check) => userspace::check::check(&meta, check)?,
            RustMiscCmd::Clippy(clippy) => userspace::check::clippy(&meta, clippy)?,
            RustMiscCmd::Fmt(fmt) => userspace::check::fmt(&meta, fmt)?,
        },
        Command::Toolchain(toolchain) => {
            toolchain::dist(&meta, &toolchain)?;
        }
    }

    Ok(())
}
