//! Private model-frame layout and storage. Allocations are pairs, never a public
//! key lookup: a computed batch carries its own tiles, object data and draw range.
use anyhow::{ensure, Result};
use std::collections::{HashMap, HashSet};

pub(super) const BATCH_TEXELS: usize = 64 * 1024 * 1024 / 16;
const MAX_STORAGE_TEXELS: usize = 128 * 1024 * 1024 / 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TileBatch {
    pub first: u32,
    pub count: u32,
    pub texels: usize,
}

/// Layout follows the published back-to-front draw stream, not instance order.
/// Offscreen instances keep their draw positions but reserve no tile cells.
#[derive(Default)]
pub(super) struct ParticleTiles {
    offsets: Vec<u32>,
    batches: Vec<TileBatch>,
}
impl ParticleTiles {
    pub fn pack(models: &[(u32, bool)], draw_order: &[u32]) -> Self {
        assert_eq!(models.len(), draw_order.len());
        let mut offsets = vec![0; models.len()];
        let mut seen = vec![false; models.len()];
        let mut batches = Vec::new();
        let mut batch = TileBatch {
            first: 0,
            count: 0,
            texels: 0,
        };
        for (draw, &index) in draw_order.iter().enumerate() {
            let index = index as usize;
            assert!(!seen[index], "model draw order must be a permutation");
            seen[index] = true;
            let (resolution, visible) = models[index];
            assert!((8..=64).contains(&resolution));
            let cells = if visible {
                (resolution * resolution) as usize
            } else {
                0
            };
            if batch.texels + cells > BATCH_TEXELS {
                batches.push(batch);
                batch = TileBatch {
                    first: draw as u32,
                    count: 0,
                    texels: 0,
                };
            }
            offsets[index] = batch.texels as u32;
            batch.count += 1;
            batch.texels += cells;
        }
        if batch.count > 0 {
            batches.push(batch);
        }
        Self { offsets, batches }
    }

    pub fn offsets(&self) -> &[u32] {
        &self.offsets
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BatchIdentity {
    Particles(usize),
    AttachedApples(u32),
    FallenApples,
}

#[derive(Clone)]
struct Allocation<B> {
    buffer: B,
    capacity: usize,
}
#[derive(Clone)]
struct StoragePair<B> {
    tiles: Allocation<B>,
    objects: Allocation<B>,
}

pub(super) struct ModelPixelBatch<B> {
    pub range: TileBatch,
    pub tiles: B,
    pub objects: B,
}
impl<B> ModelPixelBatch<B> {
    /// Only successful compute can publish a draw, using these exact allocations
    /// and range. The native adapter prepares descriptors here, before render-pass
    /// entry; deterministic tests observe the same publication seam without Vulkan.
    pub fn publish<D>(
        self,
        compute: impl FnOnce(&Self) -> Result<()>,
        draw: impl FnOnce(&Self) -> Result<D>,
    ) -> Result<D> {
        compute(&self)?;
        draw(&self)
    }
}

struct FrameSlot<B> {
    batches: HashMap<BatchIdentity, StoragePair<B>>,
    used: HashSet<BatchIdentity>,
}
impl<B> Default for FrameSlot<B> {
    fn default() -> Self {
        Self {
            batches: HashMap::new(),
            used: HashSet::new(),
        }
    }
}

/// The allocator is an internal seam: Vulkan buffers in production, identifiable
/// in-memory allocations in deterministic lifecycle tests. Fence readiness is an
/// input from FrameManager, not something this storage tries to establish.
pub(super) struct ModelPixelStorage<B> {
    frames: Vec<FrameSlot<B>>,
    current: Option<usize>,
}
impl<B> Default for ModelPixelStorage<B> {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            current: None,
        }
    }
}
impl<B: Clone> ModelPixelStorage<B> {
    pub fn begin_frame(&mut self, frame: usize) {
        self.frames
            .resize_with(self.frames.len().max(frame + 1), FrameSlot::default);
        let slot = &mut self.frames[frame];
        // Preserve last-use reuse/retirement cadence. Never touch another slot's
        // buffers, even if this frame has no models or a smaller resolution.
        slot.batches.retain(|key, _| slot.used.contains(key));
        slot.used.clear();
        self.current = Some(frame);
    }

    pub fn frame_slot(&self) -> usize {
        self.current
            .expect("begin model frame after waiting for its fence")
    }

    pub fn particles(
        &mut self,
        layout: &ParticleTiles,
        mut allocate: impl FnMut(usize) -> Result<B>,
    ) -> Result<Vec<ModelPixelBatch<B>>> {
        layout
            .batches
            .iter()
            .enumerate()
            .map(|(index, &batch)| {
                self.batch(
                    BatchIdentity::Particles(index),
                    batch,
                    layout.offsets.len(),
                    &mut allocate,
                )
            })
            .collect()
    }

    pub fn attached_apples(
        &mut self,
        tree: u32,
        count: u32,
        resolution: u32,
        allocate: impl FnMut(usize) -> Result<B>,
    ) -> Result<Option<ModelPixelBatch<B>>> {
        self.apples(
            BatchIdentity::AttachedApples(tree),
            count,
            resolution,
            allocate,
        )
    }

    pub fn fallen_apples(
        &mut self,
        count: u32,
        resolution: u32,
        allocate: impl FnMut(usize) -> Result<B>,
    ) -> Result<Option<ModelPixelBatch<B>>> {
        self.apples(BatchIdentity::FallenApples, count, resolution, allocate)
    }

    fn apples(
        &mut self,
        identity: BatchIdentity,
        count: u32,
        resolution: u32,
        mut allocate: impl FnMut(usize) -> Result<B>,
    ) -> Result<Option<ModelPixelBatch<B>>> {
        ensure!(
            (8..=64).contains(&resolution),
            "invalid apple pixel resolution"
        );
        if count == 0 {
            return Ok(None);
        }
        let texels = (count as usize)
            .checked_mul((resolution * resolution) as usize)
            .ok_or_else(|| anyhow::anyhow!("model tile allocation overflow"))?;
        self.batch(
            identity,
            TileBatch {
                first: 0,
                count,
                texels,
            },
            count as usize,
            &mut allocate,
        )
        .map(Some)
    }

    fn batch(
        &mut self,
        identity: BatchIdentity,
        range: TileBatch,
        object_count: usize,
        allocate: &mut impl FnMut(usize) -> Result<B>,
    ) -> Result<ModelPixelBatch<B>> {
        let tile_capacity = capacity(range.texels)?;
        // Four float4s per object. Particle indices are global, even in later
        // batches, so every batch must cover the complete instance address space.
        let object_capacity = capacity(
            object_count
                .checked_mul(4)
                .ok_or_else(|| anyhow::anyhow!("model object allocation overflow"))?,
        )?;
        let frame = self.frame_slot();
        let slot = &mut self.frames[frame];
        ensure!(
            !slot.used.contains(&identity),
            "model batch prepared twice in one frame"
        );
        let previous = slot.batches.get(&identity);
        let mut allocation =
            |previous: Option<&Allocation<B>>, required| -> Result<Allocation<B>> {
                if let Some(previous) = previous.filter(|a| a.capacity >= required) {
                    return Ok(previous.clone());
                }
                Ok(Allocation {
                    buffer: allocate(required)?,
                    capacity: required,
                })
            };
        let pair = StoragePair {
            tiles: allocation(previous.map(|p| &p.tiles), tile_capacity)?,
            objects: allocation(previous.map(|p| &p.objects), object_capacity)?,
        };
        let prepared = ModelPixelBatch {
            range,
            tiles: pair.tiles.buffer.clone(),
            objects: pair.objects.buffer.clone(),
        };
        slot.batches.insert(identity, pair);
        slot.used.insert(identity);
        Ok(prepared)
    }
}

fn capacity(texels: usize) -> Result<usize> {
    let capacity = texels
        .max(1)
        .checked_next_power_of_two()
        .ok_or_else(|| anyhow::anyhow!("model tile allocation overflow"))?;
    ensure!(
        capacity <= MAX_STORAGE_TEXELS,
        "model tile batch exceeds portable storage-buffer range"
    );
    Ok(capacity)
}
