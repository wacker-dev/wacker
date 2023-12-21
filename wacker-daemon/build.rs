use std::process::Command;

fn main() {
    set_commit_info_for_rustc();
}

fn set_commit_info_for_rustc() {
    let output = match Command::new("git")
        .arg("log")
        .arg("-1")
        .arg("--date=short")
        .arg("--format=%H %h %cd")
        .output()
    {
        // Something like: 1d3b80a2765e46a51290819b82f772dff09c2c49 1d3b80a 2023-12-21
        Ok(output) if output.status.success() => String::from_utf8(output.stdout).unwrap(),
        _ => return,
    };
    let mut parts = output.split_whitespace().skip(1);
    println!(
        "cargo:rustc-env=WACKER_VERSION_INFO={} ({} {})",
        env!("CARGO_PKG_VERSION"),
        parts.next().unwrap(),
        parts.next().unwrap()
    );
}
