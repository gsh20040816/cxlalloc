use rkyv::Archive;
use rkyv::Deserialize;
use rkyv::Serialize;

#[derive(Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub enum Command {
    Insert(Insert),
}

impl Command {
    pub fn parse(line: &str) -> Option<Self> {
        Insert::parse(line).map(Self::Insert)
    }
}

#[derive(Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct Insert {
    table: String,
    key: String,
    record: Vec<Pair>,
}

impl Insert {
    pub fn parse(line: &str) -> Option<Self> {
        let line = line.strip_prefix("INSERT ")?;

        let (table, line) = line.split_once(' ')?;
        let (key, mut line) = line.split_once(' ')?;

        let mut record = Vec::new();

        line.strip_prefix("[ ")?;

        loop {
            let (key, rest) = line.split_once('=')?;
            line = rest;

            let next = line.find(" field");
            let value = line[..next.or_else(|| line.find(" ]"))?].to_owned();

            record.push(Pair {
                key: key.to_owned(),
                value,
            });

            match next {
                None => break,
                Some(index) => line = &line[index + 1..],
            }
        }

        Some(Self {
            table: table.to_owned(),
            key: key.to_owned(),
            record,
        })
    }
}

#[derive(Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct Pair {
    key: String,
    value: String,
}
