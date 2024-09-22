use std::process::Command;

use sp1_helper::build_program;

fn build_contract_abi(path: &str) {
    let constracts_dir = std::path::Path::new(path);
    print!("Building contracts in {:#?}", constracts_dir.as_os_str());

    let mut command = Command::new("/home/john/.foundry/bin/forge");
    command.arg("build").current_dir(constracts_dir);
    command.status().expect("Failed to forge build");

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
    build_contract_abi("../contracts");
    build_program("../program");
}
