use crate::builder::{
    ChunkModifyStats, VOXEL_TYPE_CHERRY_WOOD, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMISSIVE,
    VOXEL_TYPE_OAK_WOOD, VOXEL_TYPE_ROCK, VOXEL_TYPE_SAND, VOXEL_TYPE_STUCCO,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BackpackVoxel {
    Dirt,
    Sand,
    Stucco,
    CherryWood,
    OakWood,
    Rock,
    Emissive,
}

impl BackpackVoxel {
    const ALL: [Self; 7] = [
        Self::Dirt,
        Self::Sand,
        Self::Stucco,
        Self::CherryWood,
        Self::OakWood,
        Self::Rock,
        Self::Emissive,
    ];

    pub(super) fn voxel_type(self) -> u32 {
        match self {
            Self::Dirt => VOXEL_TYPE_DIRT,
            Self::Sand => VOXEL_TYPE_SAND,
            Self::Stucco => VOXEL_TYPE_STUCCO,
            Self::CherryWood => VOXEL_TYPE_CHERRY_WOOD,
            Self::OakWood => VOXEL_TYPE_OAK_WOOD,
            Self::Rock => VOXEL_TYPE_ROCK,
            Self::Emissive => VOXEL_TYPE_EMISSIVE,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Dirt => "Dirt",
            Self::Sand => "Sand",
            Self::Stucco => "Stucco",
            Self::CherryWood => "Cherry wood",
            Self::OakWood => "Oak wood",
            Self::Rock => "Rock",
            Self::Emissive => "Emissive",
        }
    }

    pub(super) fn color_rgb(self) -> [u8; 3] {
        match self {
            Self::Dirt => [178, 124, 80],
            Self::Sand => [229, 204, 126],
            Self::Stucco => [209, 189, 128],
            Self::CherryWood => [219, 128, 152],
            Self::OakWood => [159, 110, 70],
            Self::Rock => [168, 176, 190],
            Self::Emissive => crate::lighting::EMISSIVE_VOXEL_COLOR_RGB8,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Dirt => 0,
            Self::Sand => 1,
            Self::Stucco => 2,
            Self::CherryWood => 3,
            Self::OakWood => 4,
            Self::Rock => 5,
            Self::Emissive => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VoxelBackpackEntry {
    pub(super) voxel: BackpackVoxel,
    pub(super) count: u32,
}

#[derive(Debug, Default)]
pub(super) struct VoxelBackpack {
    counts: [u32; BackpackVoxel::ALL.len()],
}

impl VoxelBackpack {
    fn count(&self, voxel: BackpackVoxel) -> u32 {
        self.counts[voxel.index()]
    }

    pub(super) fn snapshot(&self) -> [VoxelBackpackEntry; BackpackVoxel::ALL.len()] {
        BackpackVoxel::ALL.map(|voxel| VoxelBackpackEntry {
            voxel,
            count: self.count(voxel),
        })
    }

    pub(super) fn deposit(&mut self, voxel: BackpackVoxel, amount: u32) {
        let count = &mut self.counts[voxel.index()];
        *count = count.saturating_add(amount);
    }

    pub(super) fn deposit_removed(&mut self, stats: &ChunkModifyStats) {
        for voxel in BackpackVoxel::ALL {
            self.deposit(voxel, stats.count_removed(voxel.voxel_type()));
        }
    }

    pub(super) fn withdraw(&mut self, voxel: BackpackVoxel, amount: u32) {
        let count = &mut self.counts[voxel.index()];
        *count = count.saturating_sub(amount);
    }

    pub(super) fn first_available(&self) -> Option<(BackpackVoxel, u32)> {
        BackpackVoxel::ALL.into_iter().find_map(|voxel| {
            let count = self.count(voxel);
            (count > 0).then_some((voxel, count))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BackpackVoxel, VoxelBackpack};
    use crate::builder::{
        ChunkModifyStats, VOXEL_TYPE_DIRT, VOXEL_TYPE_EMISSIVE, VOXEL_TYPE_ROCK, VOXEL_TYPE_STUCCO,
    };

    #[test]
    fn deposits_and_withdrawals_are_saturating() {
        let mut backpack = VoxelBackpack::default();
        backpack.deposit(BackpackVoxel::Dirt, u32::MAX);
        backpack.deposit(BackpackVoxel::Dirt, 1);
        assert_eq!(backpack.count(BackpackVoxel::Dirt), u32::MAX);

        backpack.withdraw(BackpackVoxel::Dirt, u32::MAX);
        backpack.withdraw(BackpackVoxel::Dirt, 1);
        assert_eq!(backpack.count(BackpackVoxel::Dirt), 0);
    }

    #[test]
    fn removed_voxels_are_deposited_by_semantic_type() {
        let mut stats = ChunkModifyStats::default();
        stats.removed_counts[VOXEL_TYPE_DIRT as usize] = 3;
        stats.removed_counts[VOXEL_TYPE_STUCCO as usize] = 5;
        stats.removed_counts[VOXEL_TYPE_ROCK as usize] = 7;
        stats.removed_counts[VOXEL_TYPE_EMISSIVE as usize] = 11;
        let mut backpack = VoxelBackpack::default();

        backpack.deposit_removed(&stats);

        assert_eq!(backpack.count(BackpackVoxel::Dirt), 3);
        assert_eq!(backpack.count(BackpackVoxel::Stucco), 5);
        assert_eq!(backpack.count(BackpackVoxel::Rock), 7);
        assert_eq!(backpack.count(BackpackVoxel::Emissive), 11);
        assert_eq!(backpack.count(BackpackVoxel::Sand), 0);
    }

    #[test]
    fn placement_uses_the_first_available_material_in_canonical_order() {
        let mut backpack = VoxelBackpack::default();
        backpack.deposit(BackpackVoxel::Rock, 9);
        backpack.deposit(BackpackVoxel::Sand, 4);

        assert_eq!(backpack.first_available(), Some((BackpackVoxel::Sand, 4)));
        assert_eq!(
            BackpackVoxel::Emissive.color_rgb(),
            crate::lighting::EMISSIVE_VOXEL_COLOR_RGB8
        );
    }
}
