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
    Test(TestKernel),
    BuildIso(BuildIso),
    Kernel(Kernel),
    Userspace(Userspace),
    Toolchain(Toolchain),
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "run")]
#[argh(description = "Run the kernel")]
pub struct RunKernel {
    #[argh(switch, long = "gdb")]
    #[argh(description = "run with gdb")]
    pub gdb: bool,

    #[argh(switch, long = "no-graphics")]
    #[argh(description = "disable graphics")]
    pub no_graphics: bool,

    #[argh(switch, long = "no-disk")]
    #[argh(description = "disable filesystem disk")]
    pub no_disk: bool,

    #[argh(positional)]
    pub extra: Vec<String>,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "test")]
#[argh(description = "Test the kernel")]
pub struct TestKernel {
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
#[argh(subcommand, name = "kernel")]
#[argh(description = "Run rust commands on the kernel")]
pub struct Kernel {
    #[argh(subcommand)]
    pub cmd: RustMiscCmd,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "userspace")]
#[argh(description = "Run rust commands on the userspace programs")]
pub struct Userspace {
    #[argh(subcommand)]
    pub cmd: RustMiscCmd,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand)]
pub enum RustMiscCmd {
    Build(Build),
    Check(Check),
    Clippy(Clippy),
    Fmt(Fmt),
}

#[derive(FromArgs, Debug, Default)]
#[argh(subcommand, name = "build")]
#[argh(description = "Run rust build command")]
pub struct Build {
    #[argh(positional)]
    pub extra: Vec<String>,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "check")]
#[argh(description = "Run rust check command")]
pub struct Check {
    #[argh(positional)]
    pub extra: Vec<String>,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "clippy")]
#[argh(description = "Run rust clippy command")]
pub struct Clippy {
    #[argh(positional)]
    pub extra: Vec<String>,
}

#[derive(FromArgs, Debug)]
#[argh(subcommand, name = "fmt")]
#[argh(description = "Run rust fmt command")]
pub struct Fmt {
    #[argh(positional)]
    pub extra: Vec<String>,
}

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
