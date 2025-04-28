use std::io;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin().lock();
    let cli = serde_json::from_reader::<_, cxlalloc_bench::worker::Config>(stdin)?;
    cli.run()
}
