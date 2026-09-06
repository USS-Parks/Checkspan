use std::process::Command;

fn checkspan() -> Command {
    Command::new(env!("CARGO_BIN_EXE_checkspan"))
}

#[test]
fn version_flag_prints_package_version() {
    let out = checkspan().arg("--version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout.trim(),
        format!("checkspan {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_flag_prints_usage() {
    let out = checkspan().arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Usage: checkspan"));
    assert!(stdout.contains("Work you can verify."));
}

#[test]
fn unknown_flag_is_a_usage_error() {
    let out = checkspan().arg("--no-such-flag").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!out.stderr.is_empty());
}
