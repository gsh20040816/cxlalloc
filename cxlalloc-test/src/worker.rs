use core::cell::Cell;
use core::ptr;
use core::ptr::NonNull;
use std::mem;
use std::sync::OnceLock;

use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;

use cxlalloc::Raw;
use cxlalloc::thread;
use cxlalloc_test::Request;
use cxlalloc_test::Response;
use ipc_channel::ipc::IpcError;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::IpcReceiver;
use ipc_channel::ipc::IpcSender;

static RAW: OnceLock<cxlalloc::Raw> = OnceLock::new();

thread_local! {
    static ID: Cell<u16> = const { Cell::new(0) };
}

fn handle(_: libc::c_int, info: *const libc::siginfo_t, _: *const libc::c_void) {
    let address = unsafe { info.read().si_addr() };

    if RAW
        .get()
        .unwrap()
        .map(unsafe { cxlalloc::thread::Id::new(ID.get()) }, address)
    {
        return;
    }

    unsafe {
        let mut action = mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }
}

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
    let mut action = unsafe { mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = handle as usize;
    action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER;

    unsafe {
        libc::sigaction(libc::SIGSEGV, &action, ptr::null_mut());
    }

    env_logger::init();

    let cli = Cli::parse();
    let id = cli.id;
    let worker = Worker::new(cli)?;

    worker
        .run()
        .with_context(|| anyhow!("Worker {} failure", id))
}

struct Worker {
    id: u16,
    tx: IpcSender<Response>,
    rx: IpcReceiver<Request>,
    raw: &'static Raw,
}

impl Worker {
    fn run(self) -> anyhow::Result<()> {
        let mut allocator = self
            .raw
            .allocator::<(), ()>(unsafe { thread::Id::new(self.id) });

        loop {
            let request = match self.rx.recv() {
                Ok(request) => request,
                Err(IpcError::Disconnected) => return Ok(()),
                Err(error) => return Err(error).context("IPC error"),
            };

            log::info!("[{}]: receive {:x?}", self.id, request);

            match request {
                Request::Handshake => unreachable!("Protocol error"),
                Request::Allocate { id, size } => {
                    let size = size as usize;
                    let pointer = allocator.allocate_untyped(size).cast::<u64>();
                    unsafe { std::slice::from_raw_parts_mut(pointer, size / size_of::<u64>()) }
                        .fill(id);

                    let pointer = NonNull::new(pointer).unwrap();
                    let offset = allocator.pointer_to_offset(pointer.cast()) as u64;

                    self.tx.send(Response::Allocate { offset })?;
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

                    unsafe { allocator.free_untyped(pointer.cast()) };

                    self.tx.send(Response::Free)?;
                }
                Request::Load { id: _, offset } => {
                    let pointer = allocator.offset_to_pointer(offset as usize).cast::<u64>();
                    self.tx.send(Response::Load {
                        value: unsafe { pointer.read() },
                    })?;
                }
            }
        }
    }

    fn new(cli: Cli) -> anyhow::Result<Self> {
        let tx = IpcSender::<Response>::connect(cli.socket)?;
        let (server, socket) = IpcOneShotServer::<Request>::new()?;
        tx.send(Response::Handshake { socket })?;

        let (rx, Request::Handshake) = server.accept()? else {
            panic!("Expected handshake")
        };

        ID.set(cli.id);
        let raw = RAW.get_or_init(|| {
            cxlalloc::raw::Raw::builder()
                .backend(
                    cxlalloc::raw::Backend::builder()
                        .backend(cxlalloc::raw::backend::Shm)
                        .build(),
                )
                .free(false)
                .thread_count(cli.count)
                .size_small(cli.size)
                .build(&cli.name)
                .unwrap()
        });

        Ok(Self {
            id: cli.id,
            tx,
            rx,
            raw,
        })
    }
}
