use std::collections::HashMap;
use std::path::PathBuf;

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

struct Child {
    handle: std::process::Child,
    tx: IpcSender<Request>,
    rx: IpcReceiver<Response>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
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

        children.insert(id, Child { handle, tx, rx });
    }

    Ok(())
}
