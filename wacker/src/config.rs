use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use std::path::PathBuf;

static MAIN_DIR: OnceCell<PathBuf> = OnceCell::new();
static SOCK_PATH: OnceCell<PathBuf> = OnceCell::new();
static LOGS_DIR: OnceCell<PathBuf> = OnceCell::new();
static DB_PATH: OnceCell<PathBuf> = OnceCell::new();

fn get_main_dir() -> Result<&'static PathBuf> {
    MAIN_DIR.get_or_try_init(|| -> Result<PathBuf> {
        match dirs::home_dir() {
            Some(home_dir) => Ok(home_dir.join(".wacker")),
            None => Err(anyhow!("can't get home dir")),
        }
    })
}

pub fn get_sock_path() -> Result<&'static PathBuf> {
    SOCK_PATH.get_or_try_init(|| -> Result<PathBuf> { Ok(get_main_dir()?.join("wacker.sock")) })
}

pub fn get_logs_dir() -> Result<&'static PathBuf> {
    LOGS_DIR.get_or_try_init(|| -> Result<PathBuf> { Ok(get_main_dir()?.join("logs")) })
}

pub fn get_db_path() -> Result<&'static PathBuf> {
    DB_PATH.get_or_try_init(|| -> Result<PathBuf> { Ok(get_main_dir()?.join("db")) })
}
