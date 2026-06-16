fn main() {
    // Embed the short git hash at compile time so the health endpoint
    // and version displays show the actual commit instead of "dev".
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        if output.status.success() {
            let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
            println!("cargo:rustc-env=GIT_HASH={hash}");
        }
    }
    // Re-run if the git HEAD changes (new commits).
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
