use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../frontend/src");
    println!("cargo:rerun-if-changed=../../frontend/index.html");
    println!("cargo:rerun-if-changed=../../frontend/package.json");

    let frontend_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("frontend");

    let status = Command::new("pnpm")
        .arg("build")
        .current_dir(&frontend_dir)
        .status()
        .expect("Failed to run pnpm build");

    if !status.success() {
        panic!("Frontend build failed");
    }
}
