use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;

use cxlalloc_test::Request;
use cxlalloc_test::Response;
use cxlalloc_test::trace;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::IpcReceiver;
use ipc_channel::ipc::IpcSender;

#[derive(Parser)]
struct Cli {
    #[clap(short, long, default_value = "test")]
    name: String,

    #[clap(short, long, default_value = "target/debug/cxlalloc-test-worker")]
    path: PathBuf,

    #[clap(subcommand)]
    workload: Workload,
}

#[derive(Parser)]
enum Workload {
    Trace { path: PathBuf },
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match &cli.workload {
        Workload::Trace { path } => {
            let paths = if path.is_dir() {
                path.read_dir()?
                    .map(Result::unwrap)
                    .filter(|entry| entry.metadata().unwrap().is_file())
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>()
            } else if path.is_file() {
                vec![path.clone()]
            } else {
                unimplemented!()
            };

            let tests = paths
                .into_iter()
                .map(|path| {
                    let data = std::fs::read_to_string(&path)
                        .with_context(|| anyhow!("Failed to read {}", path.display()))
                        .unwrap();
                    (path, data)
                })
                .map(|(path, data)| {
                    toml::from_str::<trace::Test>(&data)
                        .with_context(|| anyhow!("Failed to parse {} as TOML", path.display()))
                        .map(|trace| (path, trace))
                })
                .collect::<Result<Vec<_>, _>>()?;

            for (path, test) in tests {
                eprintln!("Running {}...", path.display());

                let coordinator = Coordinator::new(&cli, &test)?;

                coordinator
                    .run(test.requests)
                    .context("Coordinator failure")?;
            }
        }
    };

    Ok(())
}

struct Coordinator {
    children: HashMap<usize, Child>,
    by_offset: HashMap<u64, Allocation>,
    by_id: HashMap<u64, Allocation>,
}

impl Coordinator {
    fn run(mut self, trace: Vec<trace::Request>) -> anyhow::Result<()> {
        for request in trace {
            let (thread, request) = match request {
                trace::Request::Allocate { thread, id, size } => {
                    (thread, Request::Allocate { id, size })
                }
                trace::Request::Free { thread, id } => {
                    let allocation = &self.by_id[&id];
                    (thread, Request::Free {
                        id,
                        size: allocation.size,
                        offset: allocation.offset,
                    })
                }
                trace::Request::Load { thread, id } => {
                    let allocation = &self.by_id[&id];
                    (thread, Request::Load {
                        id,
                        offset: allocation.offset,
                    })
                }
            };

            self.send(thread as usize, request)?;
        }
        Ok(())
    }

    fn new(cli: &Cli, test: &trace::Test) -> anyhow::Result<Self> {
        for entry in std::fs::read_dir("/dev/shm")?
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().unwrap().is_file())
        {
            let name = entry.file_name().into_string().unwrap();
            if name.starts_with(&cli.name) {
                std::fs::remove_file(entry.path())?;
            }
        }

        let mut children = HashMap::new();

        for id in 0..test.count {
            let (server, socket) = IpcOneShotServer::<Response>::new()?;

            let handle = std::process::Command::new(&cli.path)
                .arg("--size")
                .arg(test.size.to_string())
                .arg("--name")
                .arg(&cli.name)
                .arg("--count")
                .arg(test.count.to_string())
                .arg("--socket")
                .arg(socket)
                .arg("--id")
                .arg(id.to_string())
                .spawn()?;

            let (rx, Response::Handshake { socket }) = server.accept()? else {
                panic!("Expected handshake")
            };

            let tx = IpcSender::connect(socket)?;
            tx.send(Request::Handshake)?;

            log::info!("[C]: connected to {}", id);
            children.insert(id, Child {
                _handle: handle,
                tx,
                rx,
            });
        }

        Ok(Self {
            children,
            by_offset: HashMap::new(),
            by_id: HashMap::new(),
        })
    }

    fn send(&mut self, thread: usize, request: Request) -> anyhow::Result<()> {
        log::info!("[C]: sending request to {}: {:x?}", thread, request);

        self.children[&thread]
            .tx
            .send(request.clone())
            .with_context(|| anyhow!("Failed to send request to {}: {:?}", thread, request))?;

        let response = self.children[&thread]
            .rx
            .recv()
            .with_context(|| anyhow!("Failed to receive response from {}", thread))?;
        log::info!("[C]: received response from {}: {:x?}", thread, response);

        match (request, response) {
            (Request::Allocate { id, size }, Response::Allocate { offset }) => {
                assert!(
                    self.by_offset
                        .insert(offset, Allocation { id, size, offset })
                        .is_none()
                );

                assert!(
                    self.by_id
                        .insert(id, Allocation { id, size, offset })
                        .is_none()
                );
            }
            (
                Request::Free {
                    id,
                    size: _,
                    offset: _,
                },
                Response::Free,
            ) => {
                let allocation = self.by_id.remove(&id).unwrap();
                assert_eq!(
                    allocation,
                    self.by_offset.remove(&allocation.offset).unwrap(),
                );
            }
            (Request::Load { id, offset: _ }, Response::Load { value }) => {
                let allocation = self.by_id[&id];
                assert_eq!(allocation, self.by_offset[&allocation.offset]);
                assert_eq!(allocation.id, value);
            }
            (request, response) => unreachable!("Protocol error: {:?} -> {:?}", request, response),
        }

        Ok(())
    }
}

struct Child {
    _handle: std::process::Child,
    tx: IpcSender<Request>,
    rx: IpcReceiver<Response>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Allocation {
    id: u64,
    size: u64,
    offset: u64,
}
