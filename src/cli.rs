use serde::Deserialize;
use structopt::StructOpt;
use std::fs;
use std::error::Error;

#[derive(Debug, StructOpt)]
pub struct Cli {
    #[structopt(long = "creds")]
    pub creds: String,
}

#[derive(Debug, Deserialize)]
pub struct Creds {
    pub secret_key: String,
}

impl Creds {
    pub fn from_file(path: &str) -> Result<Self, Box<dyn Error>> {
        let data = fs::read_to_string(path)?;
        let mut creds: Creds = serde_json::from_str(&data)?;
        if let Some(trimmed) = creds.secret_key.strip_prefix("0x") {
            creds.secret_key = trimmed.to_string();
        }
        Ok(creds)
    }
}