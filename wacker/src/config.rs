use anyhow::{bail, Result};
use const_format::concatcp;
use std::path::PathBuf;

const MAIN_DIR: &str = ".wacker";
const SOCK_PATH: &str = concatcp!(MAIN_DIR, "/wacker.sock");
const LOGS_DIR: &str = concatcp!(MAIN_DIR, "/logs");
const DB_PATH: &str = concatcp!(MAIN_DIR, "/db");

#[derive(Clone)]
pub struct Config {
    pub sock_path: PathBuf,
    pub logs_dir: PathBuf,
    pub db_path: PathBuf,
}

impl Config {
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir();
        if home_dir.is_none() {
            bail!("can't get home dir");
        }
        let home_dir = home_dir.unwrap();
        Ok(Self {
            sock_path: home_dir.join(SOCK_PATH),
            logs_dir: home_dir.join(LOGS_DIR),
            db_path: home_dir.join(DB_PATH),
        })
    }
}
