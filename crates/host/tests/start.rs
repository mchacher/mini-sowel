use std::process::Command;

#[test]
fn the_host_program_starts_and_prints_its_name() {
    let output = Command::new(env!("CARGO_BIN_EXE_mini-sowel-host"))
        .output()
        .expect("the host program starts");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    assert_eq!(stdout.trim(), mini_sowel_core::banner());
}
