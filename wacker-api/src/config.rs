use const_format::concatcp;

pub const MAIN_DIR: &str = ".wacker";
pub const SOCK_PATH: &str = concatcp!(MAIN_DIR, "/wacker.sock");
pub const LOGS_DIR: &str = concatcp!(MAIN_DIR, "/logs");
pub const DB_PATH: &str = concatcp!(MAIN_DIR, "/db");
