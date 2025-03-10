use std::io;

fn main() {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::worker::Config>(stdin).unwrap();
    cli.run()
}
