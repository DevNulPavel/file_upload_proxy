use structopt::StructOpt;
use std::path::PathBuf;

/// App parameters
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub struct AppArguments {
    /// Config file path
    #[structopt(long, parse(from_os_str), env = "CONFIG_FILE")]
    pub config: PathBuf,
}
