//! Bounded-probe tree lookups sized from actual published facts, not a forest-wide fixed limit.
use super::raster_tree::tree_cell_hash;
use anyhow::{ensure, Context, Result};

pub const MIN_TREE_LOOKUP_CAPACITY: usize = 64;
pub const TREE_PUBLICATION_BUDGET_BYTES: usize = 256 * 1024 * 1024;

pub struct TreeLookup {
    pub table: Vec<[u32; 4]>,
    /// Slot for each input entry, in input order.
    pub slots: Vec<usize>,
}

pub fn build_tree_lookup(entries: &[[u32; 4]]) -> Result<TreeLookup> {
    ensure!(
        entries.iter().all(|entry| entry[3] != 0),
        "empty sentinel in tree lookup input"
    );
    let mut capacity = entries
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(1))
        .and_then(usize::checked_next_power_of_two)
        .context("tree lookup capacity overflow")?
        .max(MIN_TREE_LOOKUP_CAPACITY);
    loop {
        ensure!(
            capacity <= TREE_PUBLICATION_BUDGET_BYTES / 16,
            "tree lookup exceeds publication memory budget"
        );
        let mut table = vec![[0; 4]; capacity];
        let mut slots = Vec::with_capacity(entries.len());
        let mut exhausted = false;
        for &entry in entries {
            let hash = tree_cell_hash([entry[0], entry[1], entry[2]]) as usize;
            let mut found = None;
            for probe in 0..64 {
                let slot = (hash + probe) & (capacity - 1);
                if table[slot][3] == 0 {
                    found = Some(slot);
                    break;
                }
                ensure!(table[slot][..3] != entry[..3], "duplicate tree lookup cell");
            }
            let Some(slot) = found else {
                exhausted = true;
                break;
            };
            table[slot] = entry;
            slots.push(slot);
        }
        if !exhausted {
            return Ok(TreeLookup { table, slots });
        }
        capacity = capacity
            .checked_mul(2)
            .context("tree lookup growth overflow")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lookup_scales_past_former_attachment_limit_and_preserves_all_keys() {
        let entries: Vec<_> = (0..32769).map(|x| [x, 17, 23, x + 1]).collect();
        let lookup = build_tree_lookup(&entries).unwrap();
        assert!(lookup.table.len() >= entries.len() * 2);
        for (entry, slot) in entries.iter().zip(lookup.slots) {
            assert_eq!(lookup.table[slot], *entry);
        }
    }
    #[test]
    fn empty_lookup_is_a_valid_power_of_two_placeholder() {
        let lookup = build_tree_lookup(&[]).unwrap();
        assert_eq!(lookup.table.len(), MIN_TREE_LOOKUP_CAPACITY);
        assert!(lookup.table.iter().all(|entry| entry[3] == 0));
    }
    #[test]
    fn duplicate_keys_and_empty_sentinels_are_rejected() {
        assert!(build_tree_lookup(&[[1, 2, 3, 0]]).is_err());
        assert!(build_tree_lookup(&[[1, 2, 3, 1], [1, 2, 3, 2]]).is_err());
    }
}
