use super::*;

pub struct Config
{
    pub force: bool,
}

impl Config
{
    pub fn parse_cli() -> Self
    {
        let mut args = env::args();
        let mut config = Self { force: false };
        let exec = args.next().unwrap_or_default();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--force" | "-f" => config.force = true,

                "--help" => show_help(),
                arg => invalid_option(arg, &exec),
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

fn invalid_option(arg: &str, exec: &str) -> !
{
    eprintln!("{exec}: Invalid option -- '{arg}'");

    exit(1)
}
