use std::path::PathBuf;
use structopt::StructOpt;

/// Verbose level
// #[structopt(short, long, parse(from_occurrences))]
// pub verbose: u8,

/// App parameters
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub struct AppArguments {
    /// Application config file path
    #[structopt(short, long, parse(from_os_str), env = "UPLOADER_CONFIG_FILE")]
    pub config: PathBuf,
}

impl AppArguments {
    /// Выполняем валидацию переданных аргументов приложения
    pub fn validate_arguments(&self) -> Result<(), &str> {
        macro_rules! validate_argument {
            ($argument: expr, $desc: literal) => {
                if $argument == false {
                    return Err($desc);
                }
            };
        }

        validate_argument!(self.config.exists(), "Google credential file does not exist");
        validate_argument!(self.config.is_file(), "Google credential file is not a file");
        Ok(())
    }
}
