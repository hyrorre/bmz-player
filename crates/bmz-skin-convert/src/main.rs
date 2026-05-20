use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = CliArgs::parse(std::env::args().skip(1))?;
    match args.command {
        Command::LuaToJson { input, output, options } => {
            let warnings = bmz_skin::convert_lua_skin_to_json_file(&input, &output, &options)?;
            for warning in warnings {
                eprintln!("warning: {}", warning.message);
            }
            eprintln!("converted {} -> {}", input.display(), output.display());
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliArgs {
    command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    LuaToJson { input: PathBuf, output: PathBuf, options: BTreeMap<String, String> },
}

impl CliArgs {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut args = args.into_iter();
        let Some(command) = args.next() else {
            bail!("{}", help_text());
        };
        if command == "-h" || command == "--help" {
            bail!("{}", help_text());
        }
        if command != "lua-to-json" {
            bail!("unknown command `{command}`\n{}", help_text());
        }

        let Some(input) = args.next() else {
            bail!("lua-to-json requires an input .luaskin path");
        };
        let mut output = None;
        let mut options = BTreeMap::new();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--out" => {
                    let Some(path) = args.next() else {
                        bail!("--out requires a path");
                    };
                    output = Some(PathBuf::from(path));
                }
                "--option" => {
                    let Some(option) = args.next() else {
                        bail!("--option requires key=value");
                    };
                    let (key, value) = parse_option_pair(&option)?;
                    options.insert(key, value);
                }
                _ if arg.starts_with("--out=") => {
                    output = Some(PathBuf::from(arg.trim_start_matches("--out=")));
                }
                _ if arg.starts_with("--option=") => {
                    let (key, value) = parse_option_pair(arg.trim_start_matches("--option="))?;
                    options.insert(key, value);
                }
                _ => bail!("unknown argument `{arg}`"),
            }
        }

        let Some(output) = output else {
            bail!("lua-to-json requires --out <path>");
        };

        Ok(Self { command: Command::LuaToJson { input: PathBuf::from(input), output, options } })
    }
}

fn help_text() -> &'static str {
    "usage: bmz-skin-convert lua-to-json <input.luaskin> --out <output.json> [--option key=value]"
}

fn parse_option_pair(input: &str) -> Result<(String, String)> {
    let Some((key, value)) = input.split_once('=') else {
        bail!("option `{input}` must be key=value");
    };
    let key = key.trim();
    if key.is_empty() {
        bail!("option key must not be empty");
    }
    Ok((key.to_string(), value.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_lua_to_json_options() {
        let args = CliArgs::parse([
            "lua-to-json".to_string(),
            "skin.luaskin".to_string(),
            "--out".to_string(),
            "skin.json".to_string(),
            "--option".to_string(),
            "Play Side=1P".to_string(),
        ])
        .unwrap();

        assert_eq!(
            args,
            CliArgs {
                command: Command::LuaToJson {
                    input: PathBuf::from("skin.luaskin"),
                    output: PathBuf::from("skin.json"),
                    options: BTreeMap::from([("Play Side".to_string(), "1P".to_string())])
                }
            }
        );
    }
}
