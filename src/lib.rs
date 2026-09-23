use std::ffi::OsString;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Eq)]
pub enum CliAction {
    Help,
    Version,
}

pub fn parse_args(args: &[OsString]) -> Result<CliAction, String> {
    if args.len() > 1 {
        return Err(format!(
            "unexpected argument: {}",
            args[1].to_string_lossy()
        ));
    }

    let Some(argument) = args.first() else {
        return Ok(CliAction::Help);
    };

    match argument.to_str() {
        Some("-h" | "--help") => Ok(CliAction::Help),
        Some("-V" | "--version") => Ok(CliAction::Version),
        Some(_) => Err(format!(
            "unexpected argument: {}",
            argument.to_string_lossy()
        )),
        None => Err("argument is not valid Unicode".to_owned()),
    }
}

pub fn help_text() -> String {
    format!(
        "Viberaven {VERSION}\n\nUsage:\n  viberaven [OPTIONS]\n\nOptions:\n  -h, --help       Print help\n  -V, --version    Print version\n\nResearch and catalogue commands are not implemented yet."
    )
}

#[cfg(test)]
mod tests {
    use super::{CliAction, parse_args};
    use std::ffi::OsString;

    #[test]
    fn cli_handles_help_version_and_unimplemented_arguments() {
        assert_eq!(parse_args(&[]), Ok(CliAction::Help));
        assert_eq!(parse_args(&[OsString::from("-h")]), Ok(CliAction::Help));
        assert_eq!(
            parse_args(&[OsString::from("--version")]),
            Ok(CliAction::Version)
        );
        assert_eq!(
            parse_args(&[OsString::from("ingest")]),
            Err("unexpected argument: ingest".to_owned())
        );
    }
}
