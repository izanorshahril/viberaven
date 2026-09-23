use std::{env, process::ExitCode};
use viberaven::{CliAction, VERSION, help_text, parse_args};

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();

    match parse_args(&args) {
        Ok(CliAction::Help) => {
            println!("{}", help_text());
            ExitCode::SUCCESS
        }
        Ok(CliAction::Version) => {
            println!("viberaven {VERSION}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", help_text());
            ExitCode::from(2)
        }
    }
}
