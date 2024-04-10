use anyhow::Error;
use async_trait::async_trait;
use std::fs::File;
use std::io::Write;
use wasmtime_wasi::{HostOutputStream, StdoutStream, StreamError, StreamResult, Subscribe};

pub struct LogStream {
    pub output: File,
}

impl StdoutStream for LogStream {
    fn stream(&self) -> Box<dyn HostOutputStream> {
        Box::new(LogStream {
            output: self.output.try_clone().expect(""),
        })
    }

    fn isatty(&self) -> bool {
        false
    }
}

impl HostOutputStream for LogStream {
    fn write(&mut self, bytes: bytes::Bytes) -> StreamResult<()> {
        self.output
            .write_all(bytes.as_ref())
            .map_err(|e| StreamError::LastOperationFailed(Error::from(e)))
    }

    fn flush(&mut self) -> StreamResult<()> {
        self.output
            .flush()
            .map_err(|e| StreamError::LastOperationFailed(Error::from(e)))
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(1024 * 1024)
    }
}

#[async_trait]
impl Subscribe for LogStream {
    async fn ready(&mut self) {}
}
