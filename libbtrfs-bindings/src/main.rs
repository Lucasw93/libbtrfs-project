//! libbtrfs-bindings
//!
//! This is a simple script to genenerate Rust bindings for the btrfs headers from the Linux Kernel
//! tree from include/uapi/linux/btrfs.h and include/uapi/linux/btrfs_tree.h.
//!
//! dependencies:
//!     This script depends on on the following apt packages depending on the bindgen target:
//!     - x86_64-unknown-linux-gnu: libc6-dev
//!     - i686-unknown-linux-gnu: libc6-dev-i386
use reqwest::blocking::Client;
use std::{
    env,
    fs::{create_dir, read_to_string, write},
    path::{Path, PathBuf},
    process::exit,
};

mod callbacks;
mod cli;

struct BindgenTarget
{
    target: &'static str,
    width: u32,
}

const TARGET_X84_64: BindgenTarget =
    BindgenTarget { target: "x86_64-unknown-linux-gnu", width: 64 };

const TARGET_I686: BindgenTarget = BindgenTarget { target: "i686-unknown-linux-gnu", width: 32 };

fn get_version_string(client: &Client) -> reqwest::Result<String>
{
    const URL: &'static str = "https://www.kernel.org/releases.json";

    #[derive(serde::Deserialize, Debug)]
    struct KernelReleases
    {
        latest_stable: LatestStable,
    }

    #[derive(serde::Deserialize, Debug)]
    struct LatestStable
    {
        version: String,
    }

    let releases: KernelReleases = client.get(URL).send()?.json()?;

    Ok(releases.latest_stable.version)
}

fn write_btrfs_header(mut uapi_dir: PathBuf, version: &str, client: &Client)
-> reqwest::Result<()>
{
    const URL_BASE: &'static str =
        "https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git/plain";
    const BTRFS_H: &'static str = "include/uapi/linux/btrfs.h";
    const BTRFS_TREE_H: &'static str = "include/uapi/linux/btrfs_tree.h";

    eprintln!("Downloading: {BTRFS_H}\n");

    let btrfs_tree_h_url = client
        .get(format!("{URL_BASE}/{BTRFS_H}?h=v{version}"))
        .send()?
        .text()?;

    uapi_dir.push("btrfs.h");
    write(&uapi_dir, &btrfs_tree_h_url).expect("Failed to write uapi/btrfs.h");

    eprintln!("Downloading: {BTRFS_TREE_H}\n");

    let btrfs_tree_h = client
        .get(format!("{URL_BASE}/{BTRFS_TREE_H}?h=v{version}"))
        .send()?
        .text()?
        .replacen("#include <linux/btrfs.h>", "", 1);

    uapi_dir.set_file_name("btrfs_tree.h");
    write(&uapi_dir, &btrfs_tree_h).expect("Failed to write uapi/btrfs_tree.h");

    Ok(())
}

fn genenerate(target: BindgenTarget, manifest_dir: &Path, version: &str)
{
    eprintln!("Generating Bindings for target: {}", target.target);

    bindgen::Builder::default()
        .raw_line(format!(
            concat!(
                "/* bindings generated from Linux Kernel {} */\n\n",
                "#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]\n\n",
                "#[cfg(not(target_pointer_width = \"{}\"))]\n",
                "compile_error!(\"Requires {} bit architecture\");",
            ),
            version.trim(),
            target.width,
            target.width
        ))
        .clang_arg(format!("--target={}", target.target))
        .header(manifest_dir.join("wrapper.h").to_str().unwrap())
        .allowlist_item("(BTRFS|btrfs)_.*")
        .anon_fields_prefix("inner")
        .derive_default(false)
        .derive_debug(false)
        .disable_nested_struct_naming()
        .parse_callbacks(Box::new(callbacks::Callbacks))
        .prepend_enum_name(false)
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(manifest_dir.join(format!("generated_{}.rs", target.width)))
        .expect("Unable to write bindings");
}

fn main() -> reqwest::Result<()>
{
    println!("cargo::rerun-if-changed=wrapper.h");

    let client = Client::new();
    let config = cli::Config::parse_cli();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let upai_dir = manifest_dir.join("uapi");
    let version_txt = upai_dir.join("VERSION.txt");

    let mut version = get_version_string(&client)?;
    version.push('\n');

    if upai_dir.exists() {
        if !upai_dir.is_dir() {
            eprintln!("ERROR: uapi not a directory");
            exit(1);
        }

        if version_txt.exists() {
            if !config.force
                && version == read_to_string(&version_txt).expect("Couldn't read VERSION.txt")
            {
                eprintln!("Up to date: v{version}");
                exit(0);
            }
        }
    } else {
        create_dir(&upai_dir).unwrap();
    }

    write_btrfs_header(upai_dir, &version, &client)?;

    genenerate(TARGET_X84_64, &manifest_dir, &version);
    genenerate(TARGET_I686, &manifest_dir, &version);

    eprintln!("Updating VERSION.txt to Linux v{version}");
    write(&version_txt, &version).expect("Failed to write VERSION.txt");

    Ok(())
}
