use super::*;
use std::{borrow::Cow, ffi::OsStr};

#[derive(Default)]
pub struct Config
{
    pub force: bool,
}

impl Config
{
    pub fn parse_cli() -> Self
    {
        let mut config = Self::default();
        let mut args = argv::iter();

        let exec = args
            .next()
            .map_or_else(Default::default, OsStr::to_string_lossy);

        while let Some(arg) = args.next() {
            match arg.to_str().unwrap_or_default() {
                "--force" | "-f" => config.force = true,

                "--help" => show_help(),

                _ => invalid_opt(arg.to_string_lossy(), exec),
            }
        }
        config
    }
}

fn show_help() -> !
{
    eprintln!(concat!(
        "Usage: libbtrfs-bindings [OPTIONS]\n",
        "Generate rust bindings libbtrfs\n\n",
        "Options:\n\n",
        "  --force, -f           Download and generate bindings even if VERSION.txt is up to date with Linux latest_stable\n",
        "  --help                Dispay this message\n",
    ));

    exit(0)
}

fn invalid_opt(arg: Cow<str>, exec: Cow<str>) -> !
{
    eprintln!("{exec}: Invalid option -- '{arg}'");

    exit(1)
}
