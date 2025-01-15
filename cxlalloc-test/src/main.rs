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
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    for entry in std::fs::read_dir("/dev/shm")?
        .map(Result::unwrap)
        .filter(|entry| entry.file_type().unwrap().is_file())
    {
        let name = entry.file_name().into_string().unwrap();
        if name.starts_with(&cli.name) {
            std::fs::remove_file(entry.path())?;
        }
    }

    let coordinator = Coordinator::new(cli)?;

    coordinator.run().context("Coordinator failure")?;

    Ok(())
}

struct Coordinator {
    children: HashMap<usize, Child>,
    allocations: HashMap<u64, Allocation>,
}

impl Coordinator {
    fn run(mut self) -> anyhow::Result<()> {
        self.send(0, Request::Allocate {
            id: 0xdeadbeef,
            size: 1 << 20,
        })?;

        let (_, huge) = self.allocations.drain().next().unwrap();

        self.send(1, Request::Load {
            offset: huge.offset,
        })?;

        self.send(1, Request::Free {
            id: 0xdeadbeef,
            size: 1 << 20,
            offset: huge.offset,
        })?;

        Ok(())
    }

    fn new(cli: Cli) -> anyhow::Result<Self> {
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
            tx.send(Request::Handshake)?;

            log::info!("[C]: connected to {}", id);
            children.insert(id, Child { handle, tx, rx });
        }

        Ok(Self {
            children,
            allocations: HashMap::new(),
        })
    }

    fn send(&mut self, thread: usize, request: Request) -> anyhow::Result<()> {
        self.children[&thread]
            .tx
            .send(request.clone())
            .with_context(|| anyhow!("Failed to send request to {}: {:?}", thread, request))?;

        let response = self.children[&thread]
            .rx
            .recv()
            .with_context(|| anyhow!("Failed to receive response from {}", thread))?;

        match (request, response) {
            (Request::Allocate { id, size }, Response::Allocate { offset }) => {
                assert!(
                    self.allocations
                        .insert(offset, Allocation { id, size, offset })
                        .is_none()
                );
            }
            (
                Request::Free {
                    id: _,
                    size: _,
                    offset,
                },
                Response::Free,
            ) => {
                self.allocations.remove(&offset);
            }
            (Request::Load { offset }, Response::Load { value }) => {
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

struct Allocation {
    id: u64,
    size: u64,
    offset: u64,
}
