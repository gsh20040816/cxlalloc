use core::ptr::NonNull;

use clap::Parser;

use cxlalloc::thread;
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
    id: u16,

    #[clap(long)]
    count: usize,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    let tx = IpcSender::<Response>::connect(cli.socket)?;
    let (server, socket) = IpcOneShotServer::<Request>::new()?;
    tx.send(Response::Handshake { socket })?;

    let (rx, Request::Handshake) = server.accept()? else {
        panic!("Expected handshake")
    };

    let raw = cxlalloc::raw::Builder::default()
        .backend(cxlalloc::raw::backend::Shm)
        .free(false)
        .thread_count(cli.count)
        .build(&cli.name)
        .unwrap();

    let mut allocator = raw.allocator::<(), ()>(unsafe { thread::Id::new(cli.id) });

    loop {
        let request = rx.recv()?;
        log::info!("{}: receive {:?}", cli.id, request);

        match request {
            Request::Handshake => unreachable!("Protocol error"),
            Request::Allocate { id, size } => {
                let size = size as usize;
                let pointer = allocator.allocate_untyped(size).cast::<u64>();
                unsafe { std::slice::from_raw_parts_mut(pointer, size / size_of::<u64>()) }
                    .fill(id);

                let pointer = NonNull::new(pointer).unwrap();
                let offset = allocator.pointer_to_offset(pointer.cast()) as u64;

                tx.send(Response::Allocate { offset })?;
            }
            Request::Free { id, size, offset } => {
                let pointer = allocator.offset_to_pointer(offset as usize).cast::<u64>();

                assert!(
                    unsafe {
                        std::slice::from_raw_parts(
                            pointer.as_ptr(),
                            size as usize / size_of::<u64>(),
                        )
                    }
                    .iter()
                    .all(|word| *word == id)
                );

                unsafe { allocator.free_untyped(pointer.cast()) }

                tx.send(Response::Free)?;
            }
            Request::Load { offset } => {
                let pointer = allocator.offset_to_pointer(offset as usize).cast::<u64>();
                tx.send(Response::Load {
                    value: unsafe { pointer.read() },
                })?;
            }
        }
    }
}
