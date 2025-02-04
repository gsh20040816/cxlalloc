pub mod ycsb;

pub trait KeyValueStore {
    fn thread_id(&self) -> u64;
    fn put(&mut self, key: u64, value: u64);
    fn get(&self, key: u64) -> Option<u64>;
}
