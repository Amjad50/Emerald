#![feature(trim_prefix_suffix)]

mod args;
mod kernel;
mod profiler;
mod toolchain;
mod userspace;
mod utils;

use std::path::PathBuf;

use utils::NoDebug;

use crate::args::{Args, Command, RustMiscCmd};

#[derive(Debug)]
enum BuildMode {
    Release,
    Debug,
    Profile,
}

#[derive(Debug)]
struct GlobalMeta {
    build_mode: BuildMode,
    target_path: PathBuf,
    root_path: PathBuf,
    filesystem_path: PathBuf,
    meta: NoDebug<cargo_metadata::Metadata>,
}

impl GlobalMeta {
    pub fn load(build_mode: BuildMode) -> anyhow::Result<Self> {
        let metadata = cargo_metadata::MetadataCommand::new().exec().unwrap();

        let target_path = metadata.target_directory.clone().into_std_path_buf();
        let root_path = metadata.workspace_root.clone().into_std_path_buf();

        Ok(Self {
            build_mode,
            target_path,
            filesystem_path: root_path.join("filesystem"),
            root_path,
            meta: NoDebug(metadata),
        })
    }

    pub fn profile_path(&self) -> &'static str {
        match self.build_mode {
            BuildMode::Release => "release",
            BuildMode::Debug => "debug",
            BuildMode::Profile => "profile",
        }
    }

    pub fn profile_name(&self) -> &'static str {
        match self.build_mode {
            BuildMode::Release => "release",
            BuildMode::Debug => "dev",
            BuildMode::Profile => "profile",
        }
    }
}

fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();

    if args.profile && args.release {
        anyhow::bail!("You can't use both --profile and --release at the same time");
    }

    let build_mode = if args.profile {
        BuildMode::Profile
    } else if args.release {
        BuildMode::Release
    } else {
        BuildMode::Debug
    };

    let mut meta = GlobalMeta::load(build_mode)?;

    match args.cmd {
        Command::Run(run) => {
            let iso_path = kernel::iso::build_normal_iso(&meta)?;
            userspace::build_programs(&meta, Default::default())?;
            let result = kernel::run::RunConfig::new(iso_path)
                .with_serial(true)
                .with_gdb(run.gdb)
                .with_debug_port(true)
                .with_graphics(!run.no_graphics)
                .with_disk(!run.no_disk)
                .with_qmp_socket(args.profile)
                .run(&run.extra)?;

            std::process::exit(result);
        }
        Command::Test(test) => {
            let iso_path = kernel::iso::build_test_iso(&meta)?;
            let result = kernel::run::RunConfig::new(iso_path)
                .with_serial(true)
                .with_gdb(test.gdb)
                .with_debug_port(true)
                .with_graphics(false)
                .with_disk(false)
                .with_qmp_socket(args.profile)
                .run(&test.extra)?;

            let code = result >> 1;

            // custom exit code as qemu can't return 0
            if code == 1 {
                // QEMU exit code 3 means that the test succeeded
                println!("Test succeeded!");
                std::process::exit(0);
            } else {
                println!("Test failed! code: {code}");
                std::process::exit(1);
            }
        }
        Command::BuildIso(_) => {
            kernel::iso::build_normal_iso(&meta)?;
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
        Command::Profiler(profiler) => {
            if let Some(profile_mode) = &profiler.profile_mode {
                match profile_mode.as_str() {
                    "release" => meta.build_mode = BuildMode::Release,
                    "debug" => meta.build_mode = BuildMode::Debug,
                    "profile" => meta.build_mode = BuildMode::Profile,
                    _ => anyhow::bail!("Invalid profile mode: {}", profile_mode),
                }
            } else if args.release {
                meta.build_mode = BuildMode::Release;
            } else {
                meta.build_mode = BuildMode::Profile;
            }

            profiler::run(&meta, &profiler)?;
        }
    }

    Ok(())
}
