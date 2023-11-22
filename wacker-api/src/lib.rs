pub mod config;

mod module {
    tonic::include_proto!("module");
}
pub use self::module::*;
