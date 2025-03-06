use std::process::Command;

const TAILWIND_CLI: &str = "./tailwindcss";
const TAILWIND_CONFIG: &str = "tailwind.config.js";
const TAILWIND_INPUT: &str = "assets/css/tailwind.css";
const TAILWIND_OUTPUT: &str = "assets/css/index.css";

fn main() {
    // only "html" fragments are supposed to contain maud templates that would affect tailwind
    println!("cargo:rerun-if-changed=src/html");
    println!("cargo:rerun-if-changed=src/html.rs");
    println!("cargo:rerun-if-changed={TAILWIND_INPUT}");
    println!("cargo:rerun-if-changed={TAILWIND_CONFIG}");

    let mut cmd = Command::new(TAILWIND_CLI);
    cmd.args([
        "-c",
        TAILWIND_CONFIG,
        "-i",
        TAILWIND_INPUT,
        "-o",
        TAILWIND_OUTPUT,
    ]);

    if cfg!(not(debug_assertions)) {
        cmd.arg("--minify");
    }

    if !cmd.status().expect("failed to run tailwindcss").success() {
        panic!("tailwindcss failed");
    }
}
