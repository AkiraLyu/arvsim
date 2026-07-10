use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn arvsim() -> Command {
    Command::new(env!("CARGO_BIN_EXE_arvsim"))
}

fn temporary_image(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("arvsim-{name}-{}.bin", std::process::id()))
}

#[test]
fn help_exits_successfully() {
    let output = arvsim().arg("--help").output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: arvsim"));
}

#[test]
fn flat_image_stops_cleanly_at_the_step_limit() {
    let image = temporary_image("cli-step-limit");
    fs::write(&image, [0x93, 0x0f, 0xa0, 0x02]).unwrap();

    let output = arvsim()
        .args(["--format", "flat", "--platform", "bare", "--max-steps", "1"])
        .arg(&image)
        .output()
        .unwrap();
    fs::remove_file(image).unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("step limit reached"));
}

#[test]
fn missing_image_returns_a_nonzero_exit_code() {
    let image = temporary_image("missing");
    let _ = fs::remove_file(&image);

    let output = arvsim().arg(&image).output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to load image"));
}
