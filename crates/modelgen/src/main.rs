//! Model generator: each model is a Python module that builds one low-poly
//! object from a seed inside Blender. The module is the source; the GLB
//! under `assets/models` is its output, committed so the client runs
//! without Blender. This binary only finds Blender and runs
//! `blender/main.py` in it headless, so models build and test with the same
//! `cargo` commands as everything else; the generator itself is the Python
//! package under `blender/`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};

/// Where Blender is looked for after `$BLENDER`: the Windows and macOS
/// install paths, then the shell's `PATH`.
const CANDIDATES: &[&str] = &[
    r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe",
    "/Applications/Blender.app/Contents/MacOS/Blender",
    "blender",
];

fn blender() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BLENDER") {
        return Some(PathBuf::from(p));
    }
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.is_absolute() && p.is_file() || !p.is_absolute() && on_path(p))
}

fn on_path(name: &Path) -> bool {
    let Some(paths) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&paths).any(|dir| {
        let p = dir.join(name);
        p.is_file() || p.with_extension("exe").is_file()
    })
}

/// Runs `blender/main.py` headless with `args` after the `--`, forwarding
/// its output without Blender's own banner lines.
fn run(blender: &Path, args: &[String]) -> std::io::Result<ExitStatus> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("blender/main.py");
    let mut child = Command::new(blender)
        .args(["--background", "--factory-startup", "--python"])
        .arg(script)
        .arg("--")
        .args(args)
        .stdout(Stdio::piped())
        .spawn()?;
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let line = line?;
        let chatter = line.is_empty()
            || line.starts_with("Blender ")
            || line.starts_with("INFO ")
            || line.contains("| INFO")
            || line.contains("| Saved:");
        if !chatter {
            println!("{line}");
        }
    }
    child.wait()
}

fn main() -> ExitCode {
    let Some(blender) = blender() else {
        eprintln!("modelgen: no Blender found; set BLENDER to the executable");
        return ExitCode::FAILURE;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&blender, &args) {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("modelgen: cannot run {}: {e}", blender.display());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds every model twice at one seed in Blender and requires
    /// identical meshes and passing checks. Skips where Blender is absent
    /// rather than failing a machine that never builds models.
    #[test]
    fn every_model_is_deterministic_and_passes_its_checks() {
        let Some(blender) = blender() else {
            eprintln!("modelgen: no Blender found, skipping");
            return;
        };
        let status = run(&blender, &["--test".to_string()]).expect("blender runs");
        assert!(status.success(), "modelgen --test failed");
    }
}
