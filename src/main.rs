use std::process::ExitCode;

fn main() -> ExitCode {
    match tracky::cli::run_from_env() {
        Ok(code) => ExitCode::from(code as u8),
        Err(error) => {
            eprintln!("tracky: {error:#}");
            ExitCode::FAILURE
        }
    }
}
