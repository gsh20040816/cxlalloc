use clap::Parser;

use cxlalloc_test::Request;
use cxlalloc_test::Response;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::IpcSender;

#[derive(Parser)]
struct Cli {
    #[clap(long)]
    size: usize,

    /// IPC socket of coordinator
    #[clap(long)]
    socket: String,

    #[clap(long)]
    name: String,

    #[clap(long)]
    id: usize,

    #[clap(long)]
    count: usize,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    let tx = IpcSender::<Response>::connect(cli.socket)?;
    let (server, socket) = IpcOneShotServer::<Request>::new()?;
    tx.send(Response::Handshake { socket })?;

    let _raw = cxlalloc::raw::Builder::default()
        .backend(cxlalloc::raw::backend::Shm)
        .free(false)
        .thread_count(cli.count)
        .build(&cli.name)
        .unwrap();

    let (rx, Request::Handshake) = server.accept()? else {
        panic!("Expected handshake")
    };

    loop {
        let request = rx.recv()?;
        log::info!("{}: receive {:?}", cli.id, request);
    }
}
