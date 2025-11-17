use anyhow::Result;
use clap::Parser;

#[doc(hidden)]
mod cmd;
#[doc(hidden)]
mod participants;

fn main() -> Result<()> {
    bc_envelope::register_tags();
    let cli = cmd::Cli::parse();
    cli.exec()
}
