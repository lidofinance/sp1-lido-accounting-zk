use std::{path::PathBuf, process::Command};

use sp1_helper::build_program;

#[derive(PartialEq)]
enum ContractFolderNotFoundBehavior {
    Panic,
    Skip,
}

fn build_contract_abi(rel_path: &str, not_found_behavior: ContractFolderNotFoundBehavior) {
    let constracts_dir_path_buf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path);
    let constracts_dir = constracts_dir_path_buf.as_path();
    println!("Building contracts in {:#?}", constracts_dir.as_os_str());

    // Only build test_contracts if the folder exists
    if !constracts_dir.exists() {
        match not_found_behavior {
            ContractFolderNotFoundBehavior::Panic => {
                panic!("Contracts directory does not exist: {constracts_dir:?}");
            }
            ContractFolderNotFoundBehavior::Skip => {
                println!("Contracts directory {constracts_dir:?} does not exist - skipping and continuting the build");
                return;
            }
        }
    }

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

fn build_program_wrapper(rel_path: &str) {
    let abs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel_path);
    println!("cargo::rerun-if-changed={}", abs_path.canonicalize().unwrap().display());
    build_program(rel_path);
}

fn main() {
    println!("Running custom build commands");
    build_contract_abi("../../contracts", ContractFolderNotFoundBehavior::Panic);
    build_contract_abi("../../test_contracts", ContractFolderNotFoundBehavior::Skip);
    build_program_wrapper("../program");
    println!("Custom build successful");
}
