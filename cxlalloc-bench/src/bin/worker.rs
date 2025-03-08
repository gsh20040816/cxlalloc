use std::io;

fn main() {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::worker::Cli>(stdin).unwrap();
    cli.run()
}
