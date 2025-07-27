use std::{path::PathBuf, process::Stdio};

use cargo_metadata::Message;

use crate::GlobalMeta;

pub fn build_test_kernel(meta: &GlobalMeta) -> anyhow::Result<PathBuf> {
    let kernel_path = super::kernel_path(meta);

    let cargo = std::env::var("CARGO")?;

    let mut cmd = std::process::Command::new(cargo);

    cmd.current_dir(&kernel_path)
        .arg("test")
        .arg("--no-run")
        .arg("--message-format=json-render-diagnostics")
        .stdout(Stdio::piped());

    println!("[+] Running: {cmd:?}");

    let mut child = cmd.spawn()?;

    let reader = std::io::BufReader::new(child.stdout.take().ok_or(anyhow::anyhow!("No stdout"))?);

    let mut kernel_test_elf = None;

    for message in cargo_metadata::Message::parse_stream(reader) {
        match message? {
            Message::CompilerMessage(msg) => {
                msg.message
                    .rendered
                    .unwrap_or_default()
                    .lines()
                    .for_each(|line| {
                        println!("{line}");
                    });
            }
            Message::CompilerArtifact(artifact) => {
                if artifact.profile.test {
                    println!("[+] Built test: {}", artifact.target.name);
                    if let Some(ref path) = artifact.executable {
                        kernel_test_elf = Some(path.clone().into_std_path_buf());
                    }
                }
            }
            Message::BuildFinished(finished) => {
                println!(
                    "[+] Finished building the test: {}",
                    if finished.success {
                        "success"
                    } else {
                        "failure"
                    }
                )
            }
            _ => (), // Unknown message
        }
    }

    let status = child.wait()?;

    if !status.success() {
        anyhow::bail!("[-] Command failed, exit code: {:?}", status.code());
    }

    kernel_test_elf.ok_or(anyhow::anyhow!("No test executable found"))
}
