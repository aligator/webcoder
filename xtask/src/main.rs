use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(task) = args.next() else {
        eprintln!("usage: cargo tauri-dev|tauri-build [args...]");
        return ExitCode::from(2);
    };

    let status = match task.as_str() {
        "tauri-dev" => cargo_tauri("dev", args),
        "tauri-build" => cargo_tauri("build", args),
        _ => {
            eprintln!("unknown task: {task}");
            return ExitCode::from(2);
        }
    };

    match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(code.min(u8::MAX as i32) as u8),
        None => ExitCode::FAILURE,
    }
}

fn cargo_tauri(command: &str, args: impl Iterator<Item = String>) -> std::process::ExitStatus {
    Command::new("cargo")
        .arg("tauri")
        .arg(command)
        .args(args)
        .current_dir("src-tauri")
        .status()
        .expect("run cargo tauri")
}
