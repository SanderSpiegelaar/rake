use std::{collections::HashSet, process::Command};

#[test]
fn cli_reference_covers_every_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_rake"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: HashSet<_> = include_str!("../../../docs/cli-usage.md")
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .collect();
    let flags: Vec<_> = help
        .split_whitespace()
        .map(|word| word.trim_end_matches(','))
        .map(|word| word.split('[').next().unwrap())
        .filter(|word| word.starts_with('-'))
        .collect();
    assert!(!flags.is_empty(), "help must contain flags");
    for flag in flags {
        assert!(reference.contains(flag), "cli-usage.md is missing {flag}");
    }
}
