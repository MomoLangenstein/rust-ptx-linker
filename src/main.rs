#![deny(clippy::pedantic)]

use std::path::PathBuf;

use clap::Parser;

mod embedded_linker;
use embedded_linker::{Optimization, Session, Target};

#[derive(Debug, Parser)]
#[command(version)]
/// Linker for embedded code without any system dependencies
pub struct Args {
    /// Input LLVM bitcode file
    #[arg(long)]
    bitcode: Vec<PathBuf>,

    /// Input Rust rlib archives
    #[arg(long)]
    rlib: Vec<PathBuf>,

    /// Input Rust rlib archives where all global symbols should be kept
    #[arg(long)]
    whole_rlib: Vec<PathBuf>,

    /// Input files directory
    #[arg(short = 'L')]
    input_dir: Vec<PathBuf>,

    /// Target triple for which the code is compiled
    #[arg(long, default_value = "nvptx64-nvidia-cuda")]
    target: Target,

    /// The target cpu
    #[arg(long, alias = "arch")]
    target_cpu: Option<String>,

    /// The fallback arch
    #[arg(long)]
    fallback_arch: Option<String>,

    /// Write output to the filename
    #[arg(short, long)]
    output: PathBuf,

    // Enable link time optimization
    #[arg(long)]
    lto: bool,

    /// Emit debug information
    #[arg(long)]
    debug: bool,

    /// The optimization level
    #[arg(
        short = 'O',
        value_enum,
        default_value = "0",
        overrides_with = "optimization"
    )]
    optimization: Optimization,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let args = Args::parse();

    let mut linker = Session::new(args.target, args.target_cpu, args.output)?;

    for rlib in args.whole_rlib {
        linker.link_rlib(rlib, true)?;
    }

    for rlib in args.rlib {
        linker.link_rlib(rlib, false)?;
    }

    for bitcode in args.bitcode {
        linker.add_bitcode(bitcode, true)?;
    }

    linker.lto(args.optimization, true, args.debug, true)
}
