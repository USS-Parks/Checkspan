//! Command-line entry point.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

/// Top-level command line.
#[derive(Debug, Parser)]
#[command(name = "checkspan", version, about = "Work you can verify.")]
pub struct Cli {}

/// Parse `args` and run the requested command, returning the process exit code.
///
/// Exit codes: `0` success, `2` usage error (clap's convention).
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(_cli) => ExitCode::SUCCESS,
        Err(err) => {
            // `--help` and `--version` are reported by clap as errors with exit
            // code 0; `err.exit()` prints to the right stream and exits with the
            // matching code.
            err.exit()
        }
    }
}
