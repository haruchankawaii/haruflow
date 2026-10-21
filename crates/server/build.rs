fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
    println!("cargo:rerun-if-env-changed=HARUFLOW_BUILD_COMMIT");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args([
                "-c",
                &format!(
                    "safe.directory={}",
                    root.display()
                        .to_string()
                        .trim_start_matches(r"\\?\")
                        .replace('\\', "/")
                ),
            ])
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map(|value| value.trim().to_owned())
    };
    // Resolve actual Git paths so branch commits, packed refs and managed
    // worktrees all invalidate local build identity.
    let mut refs = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        refs.push(branch);
    }
    for reference in refs {
        if let Some(path) = git(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={}", root.join(path).display());
        }
    }
    let commit = std::env::var("HARUFLOW_BUILD_COMMIT")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .map(|value| value.trim().to_owned())
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=HARUFLOW_BUILD_COMMIT={commit}");
}
