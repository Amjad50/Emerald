use argh::FromArgs;

#[derive(FromArgs, Debug)]
#[argh(description = "XTask - a task runner")]
pub struct Args {
    #[argh(subcommand)]
    pub cmd: Command,

    #[argh(switch, long = "release")]
    #[argh(description = "build in release mode")]
    pub release: bool,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand)]
pub enum Command {
    Run(RunKernel),
    BuildIso(BuildIso),
    KernelCheck(KernelCheck),
    KernelClippy(KernelClippy),
    KernelFmtCheck(KernelFmtCheck),
    BuildUserspace(BuildUserspace),
    UserCheck(UserCheck),
    UserClippy(UserClippy),
    UserFmtCheck(UserFmtCheck),
    Toolchain(Toolchain),
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "run")]
#[argh(description = "Run the kernel")]
pub struct RunKernel {
    #[argh(switch, long = "gdb")]
    #[argh(description = "run with gdb")]
    pub gdb: bool,

    #[argh(positional)]
    pub extra: Vec<String>,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "build-iso")]
#[argh(description = "Build the kernel ISO")]
pub struct BuildIso {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "kernel-check")]
#[argh(description = "Check the kernel")]
pub struct KernelCheck {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "kernel-clippy")]
#[argh(description = "Run clippy on the kernel")]
pub struct KernelClippy {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "kernel-fmt-check")]
#[argh(description = "Run fmt check on the kernel")]
pub struct KernelFmtCheck {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "userspace")]
#[argh(description = "Build userspace programs into `./filesystem`")]
pub struct BuildUserspace {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "user-check")]
#[argh(description = "Check the userspace")]
pub struct UserCheck {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "user-clippy")]
#[argh(description = "Run clippy on the userspace")]
pub struct UserClippy {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "user-fmt-check")]
#[argh(description = "Run fmt check on the userspace")]
pub struct UserFmtCheck {}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "toolchain")]
#[argh(description = "Build the toolchain distribution")]
pub struct Toolchain {
    #[argh(switch, long = "install")]
    #[argh(description = "install the toolchain")]
    pub install: bool,

    #[argh(option, long = "out", short = 'o')]
    #[argh(description = "output folder to copy the dist files into")]
    pub out: Option<String>,
}
