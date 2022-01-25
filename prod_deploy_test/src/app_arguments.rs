use structopt::StructOpt;
use reqwest::Url;

/// App parameters
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub struct AppArguments {
    /// File uploading url
    #[structopt(long, env = "UPLOADER_API_URL")]
    pub uploader_api_url: Url,

    /// Token for uploading service
    #[structopt(long, env = "UPLOADER_API_TOKEN")]
    pub uploader_api_token: String,
}
