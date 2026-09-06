use std::process::ExitCode;

fn main() -> ExitCode {
    checkspan::cli::run(std::env::args_os())
}
