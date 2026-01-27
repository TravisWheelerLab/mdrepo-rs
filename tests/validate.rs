use anyhow::Result;
use assert_cmd::cargo_bin_cmd;

const COMMAND: &str = "validate";
const EMPTY_DIR: &str = "./tests/inputs/empty_dir";

// --------------------------------------------------
#[test]
fn dies_empty_dir() -> Result<()> {
    cargo_bin_cmd!()
        .args(&[COMMAND, EMPTY_DIR])
        .assert()
        .failure();
    Ok(())
}
