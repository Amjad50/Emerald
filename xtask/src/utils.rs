use std::path::Path;

pub struct NoDebug<T>(pub T);

impl<T> std::fmt::Debug for NoDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoDebug")
    }
}

impl<T> std::ops::Deref for NoDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for NoDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub fn has_changed(input: impl AsRef<Path>, output: impl AsRef<Path>) -> anyhow::Result<bool> {
    let inputs = glob::glob(input.as_ref().to_str().unwrap())?.map(|path| path.unwrap());
    let outputs = glob::glob(output.as_ref().to_str().unwrap())?.map(|path| path.unwrap());

    let input_mod = inputs
        .map(|path| path.metadata().unwrap().modified().unwrap())
        .max()
        .ok_or(anyhow::anyhow!("No input files found"))?;

    let output_mod = outputs
        .map(|path| {
            if let Ok(meta) = path.metadata() {
                meta.modified().unwrap()
            } else {
                std::time::SystemTime::UNIX_EPOCH
            }
        })
        .max()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    Ok(input_mod > output_mod)
}

pub fn copy_files(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> anyhow::Result<()> {
    let src = src.as_ref();
    let mut dst = dst.as_ref().to_owned();

    assert!(src.exists(), "Source path does not exist: {src:?}");
    assert!(!src.is_dir(), "Source path is not a file: {src:?}");

    if dst.is_dir() {
        let file_name = src.file_name().unwrap();
        dst = dst.join(file_name);
    }

    if !dst.parent().unwrap().is_dir() {
        std::fs::create_dir_all(dst.parent().unwrap())?;
    }

    if !has_changed(src, &dst)? {
        return Ok(());
    }

    println!("[+] Copying {src:?} to {dst:?}");
    std::fs::copy(src, dst)?;

    Ok(())
}

pub fn run_cmd(mut cmd: std::process::Command) -> anyhow::Result<()> {
    println!("[+] Running: {cmd:?}");

    let status = cmd.status()?;

    if !status.success() {
        anyhow::bail!("[-] Command failed, exit code: {:?}", status.code());
    }

    Ok(())
}
