use std::{path::PathBuf, process::Command};

use sp1_helper::build_program;

fn build_contract_abi(rel_path: &str) {
    let constracts_dir_path_buf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path);
    let constracts_dir = constracts_dir_path_buf.as_path();
    println!("Building contracts in {:#?}", constracts_dir.as_os_str());

    let mut command = Command::new("forge");
    command.arg("build").current_dir(constracts_dir);
    let output = command.output().expect("Failed to initiate forge build");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    println!(
        "====== Forge stdout ======\n {}\n====== Forge stdout end ======",
        stdout
    );
    if !output.status.success() {
        eprintln!(
            "====== Forge stderr ======\n {}\n====== Forge stderr end ======",
            stderr
        );
        panic!("Forge build failed: {}", output.status);
    }

    let dirs = vec![
        constracts_dir.join("src"),
        constracts_dir.join("lib"),
        constracts_dir.join("out"), // this is a bit strange, but this line actually make it work
        constracts_dir.join("foundry.toml"),
    ];
    for dir in dirs {
        if dir.exists() {
            println!("cargo::rerun-if-changed={}", dir.canonicalize().unwrap().display());
        }
    }
}

fn main() {
    println!("Running custom build commands");
    build_contract_abi("../../contracts");
    build_program("../program");
    println!("Custom build successful");
}
