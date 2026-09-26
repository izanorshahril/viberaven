use std::{env, process::ExitCode};
use viberaven::{execute, help_text};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();

    match execute(&args) {
        Ok(output) => {
            if !output.stderr.is_empty() {
                eprint!("{}", output.stderr);
            }
            print!("{}", output.stdout);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", help_text());
            ExitCode::from(2)
        }
    }
}
