use clap::CommandFactory;

use crate::cli::{Cli, CompletionsArgs};

pub fn execute(args: CompletionsArgs) {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "release-ratchet", &mut std::io::stdout());
}
