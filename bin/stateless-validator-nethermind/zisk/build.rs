//! Builds the Nethermind ZisK stateless-validator guest ELF.
//!
//! Nethermind's stateless-validator guest is a C#/.NET project — there is no
//! Rust source to compile here. This `build.rs`:
//!
//! 1. Clones (or refreshes) `NethermindEth/nethermind` at branch `feature/benchmark` (override via
//!    `NETHERMIND_REF`, `NETHERMIND_REPO_URL`, or `NETHERMIND_SRC`).
//! 2. Runs `make build` in `src/Nethermind/Nethermind.Stateless.ZiskGuest` (uses Docker + dotnet
//!    AOT → ZisK ELF under the hood).
//! 3. Optionally publishes `tools/StatelessInputGen` as a self-contained linux-x64 binary alongside
//!    the ELF.
//! 4. Copies the resulting ELF into `$OUT_DIR` and re-exports its path as `NETHERMIND_GUEST_ELF`.
//!
//! Set `SKIP_NETHERMIND_GUEST_BUILD=1` to bypass everything — useful for
//! `cargo check` or IDE indexing where only workspace metadata matters.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const ELF_NAME: &str = "stateless-validator-nethermind-zisk.elf";
const DEFAULT_REPO_URL: &str = "https://github.com/NethermindEth/nethermind.git";
const DEFAULT_REF: &str = "feature/benchmark";
const GUEST_SUBDIR: &str = "src/Nethermind/Nethermind.Stateless.ZiskGuest";
const INPUT_GEN_SUBDIR: &str = "tools/StatelessInputGen";
const INPUT_GEN_LAUNCHER: &str = "stateless-input-gen";
const INPUT_GEN_BIN_DIR: &str = "stateless-input-gen-bin";

fn main() {
    println!("cargo:rerun-if-env-changed=SKIP_NETHERMIND_GUEST_BUILD");
    println!("cargo:rerun-if-env-changed=NETHERMIND_REF");
    println!("cargo:rerun-if-env-changed=NETHERMIND_SRC");
    println!("cargo:rerun-if-env-changed=NETHERMIND_REPO_URL");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    // bin/stateless-validator-nethermind/zisk -> repo root is three levels up.
    let repo_root = manifest_dir
        .ancestors()
        .nth(3)
        .expect("expected repo root three levels above the crate manifest")
        .to_path_buf();

    let elf_path = out_dir.join(ELF_NAME);

    if env::var_os("SKIP_NETHERMIND_GUEST_BUILD").is_some() {
        println!(
            "cargo:warning=SKIP_NETHERMIND_GUEST_BUILD is set; skipping Nethermind guest build",
        );
        if !elf_path.exists() {
            fs::write(&elf_path, []).expect("failed to create placeholder ELF");
        }
        println!(
            "cargo:rustc-env=NETHERMIND_GUEST_ELF={}",
            elf_path.display()
        );
        return;
    }

    require_tool("git");
    require_tool("docker");
    require_tool("dotnet");
    require_tool("make");

    let repo_url = env_or(DEFAULT_REPO_URL, "NETHERMIND_REPO_URL");
    let git_ref = env_or(DEFAULT_REF, "NETHERMIND_REF");
    let src = env::var_os("NETHERMIND_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join(".nethermind-src"));

    sync_checkout(&src, &repo_url, &git_ref);

    let guest_dir = src.join(GUEST_SUBDIR);
    if !guest_dir.join("Makefile").is_file() {
        panic!(
            "expected {} Makefile not found — wrong branch?",
            guest_dir.display(),
        );
    }

    log(&format!("running `make build` in {}", guest_dir.display()));
    run(Command::new("make").arg("-C").arg(&guest_dir).arg("build"));

    let built_elf = guest_dir.join("bin/nethermind");
    if !built_elf.is_file() {
        panic!("`make build` did not produce {}", built_elf.display());
    }

    fs::copy(&built_elf, &elf_path).expect("failed to copy Nethermind ELF into OUT_DIR");

    // Record the commit the guest was built from. Useful for benchmark runners
    // that key result paths by Nethermind version.
    let sha =
        run_output(
            Command::new("git")
                .arg("-C")
                .arg(&src)
                .args(["rev-parse", "--short", "HEAD"]),
        )
        .unwrap_or_else(|_| "unknown".to_string());
    let version_path = out_dir.join(format!("{}.version", ELF_NAME.trim_end_matches(".elf")));
    fs::write(&version_path, format!("{sha}\n")).expect("failed to write .version file");
    log(&format!("recorded Nethermind version: {sha}"));

    let input_gen_dir = src.join(INPUT_GEN_SUBDIR);
    if input_gen_dir.is_dir() {
        publish_input_gen(&input_gen_dir, &out_dir);
    }

    log(&format!("installed {}", elf_path.display()));
    println!(
        "cargo:rustc-env=NETHERMIND_GUEST_ELF={}",
        elf_path.display()
    );
}

fn sync_checkout(src: &Path, repo_url: &str, git_ref: &str) {
    if src.join(".git").is_dir() {
        log(&format!(
            "using existing Nethermind checkout at {}",
            src.display()
        ));
        log(&format!("fetching and checking out {git_ref}"));
        run(Command::new("git")
            .arg("-C")
            .arg(src)
            .args(["fetch", "--tags", "--quiet"]));
        run(Command::new("git")
            .arg("-C")
            .arg(src)
            .args(["checkout", git_ref]));
    } else {
        if let Some(parent) = src.parent() {
            fs::create_dir_all(parent).expect("failed to create parent dir for NETHERMIND_SRC");
        }
        log(&format!(
            "cloning {repo_url} (branch {git_ref}) into {}",
            src.display()
        ));
        run(Command::new("git")
            .args([
                "clone",
                "--recurse-submodules",
                "--branch",
                git_ref,
                repo_url,
            ])
            .arg(src));
    }
}

fn publish_input_gen(input_gen_dir: &Path, out_dir: &Path) {
    log("publishing StatelessInputGen (self-contained linux-x64)");
    let publish_target = out_dir.join(INPUT_GEN_BIN_DIR);
    run(Command::new("dotnet")
        .args([
            "publish",
            "-c",
            "release",
            "-r",
            "linux-x64",
            "--self-contained",
            "true",
            "-p:RunAnalyzers=false",
            "-o",
        ])
        .arg(&publish_target)
        .arg(input_gen_dir.join("StatelessInputGen.csproj"))
        .stdout(Stdio::null()));

    let launcher = out_dir.join(INPUT_GEN_LAUNCHER);
    let launcher_contents = format!(
        "#!/usr/bin/env bash\n\
         # Launcher for the self-contained StatelessInputGen build.\n\
         exec \"$(dirname \"$0\")/{INPUT_GEN_BIN_DIR}/StatelessInputGen\" \"$@\"\n",
    );
    fs::write(&launcher, launcher_contents).expect("failed to write StatelessInputGen launcher");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&launcher)
            .expect("failed to stat launcher")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&launcher, perms).expect("failed to chmod launcher");
    }

    log(&format!("installed {}", launcher.display()));
}

fn require_tool(tool: &str) {
    let status = Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => {}
        _ => panic!("missing required tool on PATH: {tool}"),
    }
}

fn env_or(default: &str, var: &str) -> String {
    env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn run(cmd: &mut Command) {
    let rendered = format!("{cmd:?}");
    let status = cmd
        .status()
        .unwrap_or_else(|err| panic!("failed to spawn `{rendered}`: {err}"));
    if !status.success() {
        panic!("`{rendered}` exited with {status}");
    }
}

fn run_output(cmd: &mut Command) -> Result<String, String> {
    let output = cmd
        .output()
        .map_err(|err| format!("failed to spawn `{cmd:?}`: {err}"))?;
    if !output.status.success() {
        return Err(format!("`{cmd:?}` exited with {}", output.status));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn log(msg: &str) {
    println!("cargo:warning=[nethermind-guest] {msg}");
}
