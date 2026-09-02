//! Regenerates `fixtures/reference/` by running the Python reference implementation.
//!
//! Constitution III requires the parity fixtures to be *captured* from the reference rather
//! than written by hand. This harness is what makes that reproducible: it pins the reference
//! at one commit, builds an isolated virtualenv, and runs the reference's own code over every
//! case in `fixtures/inputs/`.
//!
//! It runs on demand, not in CI (research R9). The goldens are committed, so the test suite
//! stays hermetic and a reference bump shows up as a reviewable diff.
//!
//! ```text
//! cargo run -p capture-reference              # inputs, then goldens
//! cargo run -p capture-reference -- --check    # fail if the goldens are stale
//! cargo run -p capture-reference -- --case markers --case oversized
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The reference commit these goldens were captured from.
///
/// Bumping this is a deliberate act: it changes what parity *means*, so the resulting golden
/// diff is the review.
const PINNED_COMMIT: &str = "da75c532af495c00c96a8114c5e2c6536ba73260";

const REFERENCE_REPO: &str = "https://github.com/LapunovRodion/news-builder";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("capture-reference: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    /// Regenerate `fixtures/inputs/` before capturing.
    inputs: bool,
    /// Capture goldens.
    capture: bool,
    /// Fail if anything changed, rather than writing it.
    check: bool,
    /// Restrict the run to these cases.
    cases: Vec<String>,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        inputs: true,
        capture: true,
        check: false,
        cases: Vec::new(),
    };
    let mut only_inputs = false;
    let mut only_capture = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--inputs" => only_inputs = true,
            "--capture" => only_capture = true,
            "--check" => options.check = true,
            "--case" => {
                let value = args.next().ok_or("--case needs a case name")?;
                options.cases.push(value);
            }
            "-h" | "--help" => {
                println!("{}", HELP);
                std::process::exit(0);
            }
            other => return Err(format!("unrecognised argument `{other}`")),
        }
    }

    if only_inputs || only_capture {
        options.inputs = only_inputs;
        options.capture = only_capture;
    }
    Ok(options)
}

const HELP: &str = "\
capture-reference — regenerate the parity goldens from the Python reference

    --inputs          only regenerate fixtures/inputs/
    --capture         only capture fixtures/reference/
    --check           fail if the result differs from what is committed
    --case <NAME>     restrict to one case; repeatable
";

fn run() -> Result<(), String> {
    let options = parse_args()?;
    let repo_root = repo_root()?;
    let cache = repo_root.join("tools/capture-reference/.cache");
    std::fs::create_dir_all(&cache).map_err(|e| format!("creating {}: {e}", cache.display()))?;

    let reference = cache.join("news-builder");
    ensure_reference_checkout(&reference)?;
    let python = ensure_virtualenv(&cache, &reference)?;

    if options.inputs {
        println!("regenerating fixtures/inputs/");
        run_script(
            &python,
            &repo_root.join("tools/capture-reference/make_inputs.py"),
            &[],
        )?;
    }

    if options.capture {
        println!("capturing fixtures/reference/ at {PINNED_COMMIT}");
        let mut args = vec![
            "--reference".to_owned(),
            reference.display().to_string(),
            "--commit".to_owned(),
            PINNED_COMMIT.to_owned(),
        ];
        for case in &options.cases {
            args.push("--case".to_owned());
            args.push(case.clone());
        }
        run_script(
            &python,
            &repo_root.join("tools/capture-reference/capture.py"),
            &args,
        )?;
    }

    if options.check {
        check_clean(&repo_root)?;
    }

    println!("done");
    Ok(())
}

/// Locates the repository root from the manifest directory, so the harness works from anywhere.
fn repo_root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "cannot find the repository root above {}",
                manifest.display()
            )
        })
}

/// Clones the reference if it is absent, then pins it to [`PINNED_COMMIT`].
fn ensure_reference_checkout(reference: &Path) -> Result<(), String> {
    if !reference.join(".git").is_dir() {
        println!("cloning {REFERENCE_REPO}");
        run_command(
            Command::new("git")
                .args(["clone", "--quiet", REFERENCE_REPO])
                .arg(reference),
            "git clone",
        )?;
    }

    // Fetch only when the pinned commit is not already present, so a repeat run is offline.
    let has_commit = Command::new("git")
        .current_dir(reference)
        .args(["cat-file", "-e", &format!("{PINNED_COMMIT}^{{commit}}")])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !has_commit {
        println!("fetching {PINNED_COMMIT}");
        run_command(
            Command::new("git")
                .current_dir(reference)
                .args(["fetch", "--quiet", "origin"]),
            "git fetch",
        )?;
    }

    run_command(
        Command::new("git")
            .current_dir(reference)
            .args(["checkout", "--quiet", PINNED_COMMIT]),
        "git checkout",
    )
}

/// Builds the virtualenv the reference runs in, and returns its interpreter.
///
/// `lxml` is not one of the reference's own requirements; the input generator needs it to turn
/// inline Word drawings into floating anchors, which `python-docx` cannot write.
fn ensure_virtualenv(cache: &Path, reference: &Path) -> Result<PathBuf, String> {
    let venv = cache.join("venv");
    let python = venv.join("bin/python");
    let stamp = venv.join(".requirements-stamp");
    let requirements = reference.join("requirements-news-builder.txt");
    let requirements_text = std::fs::read_to_string(&requirements)
        .map_err(|e| format!("reading {}: {e}", requirements.display()))?;

    if python.is_file()
        && std::fs::read_to_string(&stamp).ok().as_deref() == Some(&requirements_text)
    {
        return Ok(python);
    }

    if !python.is_file() {
        println!("creating the virtualenv");
        run_command(
            Command::new("python3").arg("-m").arg("venv").arg(&venv),
            "python3 -m venv",
        )?;
    }

    println!("installing the reference's requirements");
    run_command(
        Command::new(&python).args(["-m", "pip", "install", "--quiet", "--upgrade", "pip"]),
        "pip upgrade",
    )?;
    run_command(
        Command::new(&python)
            .args(["-m", "pip", "install", "--quiet", "-r"])
            .arg(&requirements)
            .arg("lxml"),
        "pip install",
    )?;
    std::fs::write(&stamp, requirements_text)
        .map_err(|e| format!("writing {}: {e}", stamp.display()))?;
    Ok(python)
}

fn run_script(python: &Path, script: &Path, args: &[String]) -> Result<(), String> {
    run_command(
        Command::new(python).arg(script).args(args),
        &format!("python {}", script.display()),
    )
}

fn run_command(command: &mut Command, what: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("{what} could not start: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed with {status}"))
    }
}

/// `--check`: the goldens on disk must match what a fresh capture produces.
fn check_clean(repo_root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["status", "--porcelain", "--", "fixtures/"])
        .output()
        .map_err(|e| format!("git status could not start: {e}"))?;
    let changed = String::from_utf8_lossy(&output.stdout);
    if changed.trim().is_empty() {
        println!("fixtures are up to date");
        Ok(())
    } else {
        Err(format!(
            "fixtures are stale — re-run `cargo run -p capture-reference` and commit:\n{changed}"
        ))
    }
}
