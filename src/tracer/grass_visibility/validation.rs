//! Opt-in native checks, after the existing frame-slot fence; never used in perf runs.
use super::{culling_enabled, stem_band_mesh, FloraFrameBatch, COMMAND_WORDS};
use re_flora_vkn::Buffer;

pub(super) fn check_completed(buffer: &Buffer, batches: &[FloraFrameBatch]) {
    if batches.is_empty() {
        return;
    }
    let mut submitted = 0u64;
    let mut visible = 0u64;
    let mut bands = 0u64;
    for batch in batches {
        let count = batch.instance_count();
        let bytes = buffer
            .read_back_range(
                u64::from(batch.visibility_offset()) * 4,
                u64::from(COMMAND_WORDS + count) * 4,
            )
            .expect("read completed visibility slot");
        let words: Vec<_> = bytes
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(
            words[0],
            stem_band_mesh::grass_index_count(batch.mesh_voxel_count())
        );
        assert_eq!(&words[2..5], &[0, 0, 0]);
        let kept = words[1];
        assert!(kept <= count);
        let mut seen = vec![false; count as usize];
        for (compact, &source) in words[5..5 + kept as usize].iter().enumerate() {
            assert!(
                source < count && !seen[source as usize],
                "invalid or duplicate source identity"
            );
            seen[source as usize] = true;
            if !culling_enabled() {
                assert_eq!(source as usize, compact);
            }
        }
        if !culling_enabled() {
            assert_eq!(kept, count);
        }
        submitted += u64::from(count);
        visible += u64::from(kept);
        bands += u64::from(kept) * u64::from(batch.mesh_voxel_count());
    }
    log::info!("[GRASS_VISIBILITY_VALIDATED] source={submitted} visible={visible} bands={bands} unique=true range=true indirect=true");
}
