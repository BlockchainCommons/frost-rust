#![allow(dead_code)]

/// A macro to assert that two values are equal, printing them if they are not,
/// including newlines and indentation they may contain. This macro is useful
/// for debugging tests where you want to see the actual and expected values
/// when they do not match.
#[macro_export]
macro_rules! assert_actual_expected {
    ($actual:expr, $expected:expr $(,)?) => {
        match (&$actual, &$expected) {
            (actual_val, expected_val) => {
                if !(*actual_val == *expected_val) {
                    println!("Actual:\n{actual_val}");
                    similar_asserts::assert_eq!(actual: $actual, expected: $expected);
                }
            }
        }
    };
    ($actual:expr, $expected:expr, $($arg:tt)+) => {
        match (&$actual, &$expected) {
            (actual_val, expected_val) => {
                if !(*actual_val == *expected_val) {
                    println!("Actual:\n{actual_val}");
                    similar_asserts::assert_eq!(actual: $actual, expected: $expected, $($arg)+);
                }
            }
        }
    };
}

use std::{fs, path::Path};

use assert_cmd::{Command, cargo::cargo_bin_cmd};

/// Run the frost binary with the provided args in the given working directory.
pub fn run_frost(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = cargo_bin_cmd!("frost");
    cmd.current_dir(cwd);
    cmd.args(args);
    cmd
}

/// Load a fixture from `tests/fixtures/<name>` trimming any trailing newline.
pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    fs::read_to_string(path).unwrap().trim().to_owned()
}

pub fn registry_file(dir: &Path) -> std::path::PathBuf {
    dir.join("registry.json")
}
