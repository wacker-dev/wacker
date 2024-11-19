use crate::proto::wacker_server::Wacker;
use crate::runtime::{new_engines, Engine, ProgramMeta};
use crate::utils::generate_random_string;
use crate::{
    DeleteRequest, ListResponse, LogRequest, LogResponse, Program, ProgramResponse, RestartRequest, RunRequest,
    ServeRequest, StopRequest, PROGRAM_STATUS_ERROR, PROGRAM_STATUS_FINISHED, PROGRAM_STATUS_RUNNING,
    PROGRAM_STATUS_STOPPED, PROGRAM_TYPE_CLI, PROGRAM_TYPE_HTTP,
};
use anyhow::{anyhow, Error, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use hashbrown::HashMap;
use log::{error, info, warn};
use parking_lot::Mutex;
use rayon::prelude::*;
use sled::Db;
use std::fmt::Display;
use std::fs::{remove_file, OpenOptions};
use std::io::{ErrorKind, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    sync::{mpsc, oneshot, oneshot::error::TryRecvError},
    task, time,
};
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};
use tonic::{Request, Response, Status};

pub struct Server {
    db: Db,
    engines: HashMap<u32, Arc<dyn Engine>>,
    programs: Arc<Mutex<HashMap<String, InnerProgram>>>,
    logs_dir: PathBuf,
}

struct InnerProgram {
    id: String,
    meta: ProgramMeta,
    receiver: oneshot::Receiver<Error>,
    handler: task::JoinHandle<()>,
    status: u32,
    error: Option<Error>,
}

impl TryFrom<&mut InnerProgram> for Program {
    type Error = Error;

    fn try_from(inner: &mut InnerProgram) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: inner.id.clone(),
            path: inner.meta.path.clone(),
            program_type: inner.meta.program_type,
            status: inner.status,
            addr: inner.meta.addr.clone().unwrap_or_default(),
        })
    }
}

impl Server {
    pub async fn new<P: AsRef<Path>>(db: Db, logs_dir: P) -> Result<Self> {
        let service = Self {
            db,
            engines: new_engines()?,
            programs: Arc::new(Mutex::new(HashMap::new())),
            logs_dir: logs_dir.as_ref().to_path_buf(),
        };
        service.load_from_db().await?;

        Ok(service)
    }

    async fn load_from_db(&self) -> Result<()> {
        for data in self.db.iter() {
            let (id, bytes) = data?;
            self.run_inner(
                std::str::from_utf8(id.to_vec().as_ref())?,
                bincode::deserialize(&bytes)?,
            )
            .await?
        }
        Ok(())
    }

    async fn run_inner(&self, id: &str, meta: ProgramMeta) -> Result<()> {
        let mut programs = self.programs.lock();
        let (sender, receiver) = oneshot::channel();
        let engine = self
            .engines
            .get(&meta.program_type)
            .ok_or(anyhow!("unknown program type {}", meta.program_type))?
            .clone();

        let mut stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.logs_dir.join(id))?;
        let stdout_clone = stdout.try_clone()?;

        let id = id.to_string();
        programs.insert(
            id.clone(),
            InnerProgram {
                id: id.clone(),
                meta: meta.clone(),
                receiver,
                handler: task::spawn(async move {
                    match engine.run(meta, stdout_clone).await {
                        Ok(_) => {}
                        Err(e) => {
                            error!("running program {} error: {}", id, e);
                            if let Err(file_err) = stdout.write_fmt(format_args!("{}\n", e)) {
                                warn!("write error log failed: {}", file_err);
                            }
                            if sender.send(e).is_err() {
                                warn!("the receiver dropped");
                            }
                        }
                    }
                }),
                status: PROGRAM_STATUS_RUNNING,
                error: None,
            },
        );

        Ok(())
    }

    async fn update_db_and_run(&self, id: &str, meta: ProgramMeta) -> Result<Response<ProgramResponse>, Status> {
        match bincode::serialize(&meta) {
            Ok(bytes) => {
                self.db.insert(id, bytes).map_err(to_status)?;
                self.run_inner(id, meta).await.map_err(to_status)?;
                Ok(Response::new(ProgramResponse { id: id.to_string() }))
            }
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }

    fn get_program_keys(&self) -> Vec<String> {
        let programs = self.programs.lock();
        programs.par_iter().map(|(key, _)| key.clone()).collect()
    }
}

fn search_id(keys: &Vec<String>, id: &str) -> Result<String> {
    let positions: Vec<_> = keys.par_iter().positions(|s| s.starts_with(id)).collect();
    match positions.len() {
        0 => Err(anyhow!("program {} not found", id)),
        1 => Ok(keys[positions[0]].clone()),
        _ => Err(anyhow!(
            "ambiguous program id {}, more than one program starts with this id",
            id
        )),
    }
}

fn to_status<E: Display>(err: E) -> Status {
    Status::internal(err.to_string())
}

#[async_trait]
impl Wacker for Server {
    async fn run(&self, request: Request<RunRequest>) -> Result<Response<ProgramResponse>, Status> {
        let req = request.into_inner();

        let file_path = Path::new(&req.path);
        let name = file_path.file_stem();
        if name.is_none() {
            return Err(Status::internal(format!(
                "failed to get file name in path {}",
                req.path
            )));
        }
        let id = format!("{}-{}", name.unwrap().to_str().unwrap(), generate_random_string(7));

        info!("Execute newly added program: {} ({})", id, req.path);

        self.update_db_and_run(
            id.as_str(),
            ProgramMeta {
                path: req.path,
                program_type: PROGRAM_TYPE_CLI,
                addr: None,
                args: req.args,
            },
        )
        .await
    }

    async fn serve(&self, request: Request<ServeRequest>) -> Result<Response<ProgramResponse>, Status> {
        let req = request.into_inner();

        let file_path = Path::new(&req.path);
        let name = file_path.file_stem();
        if name.is_none() {
            return Err(Status::internal(format!(
                "failed to get file name in path {}",
                req.path
            )));
        }
        let id = format!("{}-{}", name.unwrap().to_str().unwrap(), generate_random_string(7));

        info!("Serve newly added program: {} ({})", id, req.path);

        self.update_db_and_run(
            id.as_str(),
            ProgramMeta {
                path: req.path,
                program_type: PROGRAM_TYPE_HTTP,
                addr: Option::from(req.addr),
                args: vec![],
            },
        )
        .await
    }

    async fn list(&self, _: Request<()>) -> Result<Response<ListResponse>, Status> {
        let mut reply = ListResponse { programs: vec![] };
        let mut programs = self.programs.lock();

        for (_, inner) in programs.iter_mut() {
            match inner.status {
                PROGRAM_STATUS_RUNNING if inner.handler.is_finished() => {
                    inner.status = match inner.receiver.try_recv() {
                        Ok(err) => {
                            inner.error = Option::from(err);
                            PROGRAM_STATUS_ERROR
                        }
                        Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => PROGRAM_STATUS_FINISHED,
                    };
                }
                _ => {}
            };

            reply.programs.push(inner.try_into().map_err(to_status)?);
        }

        Ok(Response::new(reply))
    }

    async fn stop(&self, request: Request<StopRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        let keys = self.get_program_keys();
        let mut programs = self.programs.lock();

        for id in req.ids {
            let id = search_id(keys.as_ref(), id.as_str()).map_err(to_status)?;

            info!("Stop the program: {}", id);

            let program = programs.get_mut(id.as_str()).unwrap();
            if !program.handler.is_finished() {
                program.handler.abort();
                program.status = PROGRAM_STATUS_STOPPED;
            }
        }
        Ok(Response::new(()))
    }

    async fn restart(&self, request: Request<RestartRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        let keys = self.get_program_keys();

        for id in req.ids {
            let id = search_id(keys.as_ref(), id.as_str()).map_err(to_status)?;

            info!("Restart the program: {}", id);

            let meta = {
                let programs = self.programs.lock();
                let program = programs.get(id.as_str()).unwrap();
                if !program.handler.is_finished() {
                    program.handler.abort();
                }
                program.meta.clone()
            };

            self.run_inner(id.as_str(), meta).await.map_err(to_status)?;
        }
        Ok(Response::new(()))
    }

    async fn delete(&self, request: Request<DeleteRequest>) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        let keys = self.get_program_keys();
        let mut programs = self.programs.lock();

        for id in req.ids {
            let id = search_id(keys.as_ref(), id.as_str()).map_err(to_status)?;

            info!("Delete the program: {}", id);

            let program = programs.get(id.as_str()).unwrap();
            if !program.handler.is_finished() {
                program.handler.abort();
            }

            if let Err(err) = remove_file(self.logs_dir.join(id.as_str())) {
                if err.kind() != ErrorKind::NotFound {
                    return Err(Status::internal(format!(
                        "failed to remove the log file for {}: {}",
                        id.as_str(),
                        err
                    )));
                }
            }

            self.db.remove(id.as_str()).map_err(to_status)?;
            programs.remove(id.as_str());
        }
        Ok(Response::new(()))
    }

    type LogsStream = Pin<Box<dyn Stream<Item = Result<LogResponse, Status>> + Send>>;

    async fn logs(&self, request: Request<LogRequest>) -> Result<Response<Self::LogsStream>, Status> {
        let req = request.into_inner();
        let keys = self.get_program_keys();
        let id = search_id(keys.as_ref(), req.id.as_str()).map_err(to_status)?;

        let mut file = File::open(self.logs_dir.join(id)).await?;
        let mut contents = String::new();
        let last_position = file.read_to_string(&mut contents).await?;
        let lines: Vec<&str> = contents.split_inclusive('\n').collect();

        let len = lines.len();
        let mut tail = req.tail as usize;
        if tail == 0 || tail > len {
            tail = len;
        }
        let content = &lines[len - tail..];

        let (tx, rx) = mpsc::channel(128);
        tx.send(Result::<_, Status>::Ok(LogResponse {
            content: content.concat(),
        }))
        .await
        .map_err(to_status)?;

        if req.follow {
            let mut stream = Box::pin(loop_stream(file, last_position));
            tokio::spawn(async move {
                while let Some(content) = stream.next().await {
                    match tx
                        .send(Result::<_, Status>::Ok(LogResponse {
                            content: content.unwrap(),
                        }))
                        .await
                    {
                        Ok(_) => {
                            // item (server response) was queued to be send to client
                        }
                        Err(_) => {
                            // output_stream was build from rx and both are dropped
                            break;
                        }
                    }
                }
            });
        }

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::LogsStream))
    }
}

fn loop_stream(mut file: File, mut last_position: usize) -> impl Stream<Item = Result<String>> {
    let mut contents = String::new();
    let mut interval = time::interval(Duration::from_millis(200));

    try_stream! {
        loop {
            contents.truncate(0);
            file.seek(SeekFrom::Start(last_position as u64)).await?;
            last_position += file.read_to_string(&mut contents).await?;
            if !contents.is_empty() {
                yield contents.clone();
            }

            interval.tick().await;
        }
    }
}
