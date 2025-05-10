use comfy_table::{Cell, Table};
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use std::{sync::Mutex, time::Duration};

pub static BENCH: Lazy<Mutex<Bench>> = Lazy::new(|| Mutex::new(Bench::new()));

#[derive(Debug)]
struct Stat {
    count: u32,
    total: Duration,
    min: Duration,
    min_idx: u32,
    max: Duration,
    max_idx: u32,
}

impl Stat {
    fn new() -> Self {
        Stat {
            count: 0,
            total: Duration::ZERO,
            min: Duration::MAX,
            min_idx: 0,
            max: Duration::ZERO,
            max_idx: 0,
        }
    }

    fn record(&mut self, d: Duration) {
        let idx = self.count + 1;
        self.count += 1;
        self.total += d;

        if d < self.min {
            self.min = d;
            self.min_idx = idx;
        }
        if d > self.max {
            self.max = d;
            self.max_idx = idx;
        }
    }

    fn avg(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / self.count
        }
    }
}

pub struct Bench {
    // IndexMap keeps insertion order
    stats: IndexMap<&'static str, Stat>,
}

impl Bench {
    pub fn new() -> Self {
        Bench {
            stats: IndexMap::new(),
        }
    }

    /// Record one sample of duration `d` under the key `name`.
    pub fn record(&mut self, name: &'static str, d: Duration) {
        self.stats.entry(name).or_insert_with(Stat::new).record(d);
    }

    /// Emit a debug-level table of avg / min@idx / max@idx / count per key,
    /// in the order each key was first recorded.
    pub fn summary(&self) {
        let mut table = Table::new();
        table.set_header(vec![
            Cell::new("Name"),
            Cell::new("Avg"),
            Cell::new("Min@Idx"),
            Cell::new("Max@Idx"),
            Cell::new("Count"),
        ]);

        for (&name, st) in &self.stats {
            table.add_row(vec![
                Cell::new(name),
                Cell::new(format!("{:?}", st.avg())),
                Cell::new(format!("{:?}@{}", st.min, st.min_idx)),
                Cell::new(format!("{:?}@{}", st.max, st.max_idx)),
                Cell::new(st.count),
            ]);
        }

        log::debug!("\n{}", table);
    }
}
