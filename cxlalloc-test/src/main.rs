use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;

use cxlalloc_test::Request;
use cxlalloc_test::Response;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::IpcReceiver;
use ipc_channel::ipc::IpcSender;

#[derive(Parser)]
struct Cli {
    #[clap(short, long, default_value_t = 1 << 34)]
    size: usize,

    #[clap(short, long, default_value = "test")]
    name: String,

    #[clap(short, long)]
    count: usize,

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

            let traces = paths
                .into_iter()
                .map(|path| {
                    let data = std::fs::read_to_string(&path)
                        .with_context(|| anyhow!("Failed to read {}", path.display()))
                        .unwrap();
                    (path, data)
                })
                .map(|(path, data)| {
                    toml::from_str::<Vec<Request>>(&data)
                        .with_context(|| anyhow!("Failed to parse {} as TOML", path.display()))
                        .map(|trace| (path, trace))
                })
                .collect::<Result<Vec<_>, _>>()?;

            for (path, trace) in traces {
                eprintln!("Running {}...", path.display());

                let coordinator = Coordinator::new(&cli)?;

                coordinator.run(trace).context("Coordinator failure")?;
            }
        }
    };

    Ok(())
}

struct Coordinator {
    children: HashMap<usize, Child>,
    allocations: HashMap<u64, Allocation>,
}

impl Coordinator {
    fn run(mut self, trace: Vec<Request>) -> anyhow::Result<()> {
        for request in trace {
            self.send(request)?;
        }
        Ok(())
    }

    fn new(cli: &Cli) -> anyhow::Result<Self> {
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

        for id in 0..cli.count {
            let (server, socket) = IpcOneShotServer::<Response>::new()?;

            let handle = std::process::Command::new(&cli.path)
                .arg("--size")
                .arg(cli.size.to_string())
                .arg("--name")
                .arg(&cli.name)
                .arg("--count")
                .arg(cli.count.to_string())
                .arg("--socket")
                .arg(socket)
                .arg("--id")
                .arg(id.to_string())
                .spawn()?;

            let (rx, Response::Handshake { socket }) = server.accept()? else {
                panic!("Expected handshake")
            };

            let tx = IpcSender::connect(socket)?;
            tx.send(Request::Handshake { thread: id as u64 })?;

            log::info!("[C]: connected to {}", id);
            children.insert(id, Child { handle, tx, rx });
        }

        Ok(Self {
            children,
            allocations: HashMap::new(),
        })
    }

    fn send(&mut self, request: Request) -> anyhow::Result<()> {
        log::info!("[C]: sending request: {:x?}", request);
        let thread = request.thread() as usize;

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
            (
                Request::Allocate {
                    thread: _,
                    id,
                    size,
                },
                Response::Allocate { offset },
            ) => {
                assert!(
                    self.allocations
                        .insert(offset, Allocation { id, size, offset })
                        .is_none()
                );
            }
            (
                Request::Free {
                    thread: _,
                    id: _,
                    size: _,
                    offset,
                },
                Response::Free,
            ) => {
                self.allocations.remove(&offset);
            }
            (Request::Load { thread: _, offset }, Response::Load { value }) => {
                assert_eq!(value, self.allocations[&offset].id);
            }
            (request, response) => unreachable!("Protocol error: {:?} -> {:?}", request, response),
        }

        Ok(())
    }
}

struct Child {
    handle: std::process::Child,
    tx: IpcSender<Request>,
    rx: IpcReceiver<Response>,
}

#[derive(Copy, Clone)]
struct Allocation {
    id: u64,
    size: u64,
    offset: u64,
}
