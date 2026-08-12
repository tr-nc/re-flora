pub(super) const DIRECT_SUN_SHADOW_SOURCE_TERRAIN: u32 = 1 << 0;
pub(super) const DIRECT_SUN_SHADOW_SOURCE_LEAF: u32 = 1 << 1;
pub(super) const DIRECT_SUN_SHADOW_SOURCE_CLOUD: u32 = 1 << 2;
pub const DIRECT_SUN_SHADOW_SOURCE_ALL: u32 = DIRECT_SUN_SHADOW_SOURCE_TERRAIN
    | DIRECT_SUN_SHADOW_SOURCE_LEAF
    | DIRECT_SUN_SHADOW_SOURCE_CLOUD;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DirectSunShadowUpdatePlan {
    reset_terrain_history: bool,
    reset_leaf_history: bool,
}

impl DirectSunShadowUpdatePlan {
    pub(super) fn reset_terrain_history(self) -> bool {
        self.reset_terrain_history
    }

    pub(super) fn reset_leaf_history(self) -> bool {
        self.reset_leaf_history
    }
}

#[derive(Debug, Default)]
pub(super) struct DirectSunShadowRuntime {
    camera_initialized: bool,
    terrain_history_valid: bool,
    leaf_history_valid: bool,
    cloud_history_valid: bool,
}

impl DirectSunShadowRuntime {
    pub(super) fn camera_update_required(&self, update_requested: bool) -> bool {
        update_requested || !self.camera_initialized
    }

    pub(super) fn mark_camera_updated(&mut self) {
        self.camera_initialized = true;
    }

    pub(super) fn invalidate_local_histories(&mut self) {
        self.terrain_history_valid = false;
        self.leaf_history_valid = false;
    }

    pub(super) fn plan_update(&self, additional_terrain_reset: bool) -> DirectSunShadowUpdatePlan {
        DirectSunShadowUpdatePlan {
            reset_terrain_history: additional_terrain_reset || !self.terrain_history_valid,
            reset_leaf_history: !self.leaf_history_valid,
        }
    }

    pub(super) fn mark_terrain_history_recorded(&mut self) {
        self.terrain_history_valid = true;
    }

    pub(super) fn mark_leaf_history_recorded(&mut self) {
        self.leaf_history_valid = true;
    }

    pub(super) fn cloud_history_reset_required(&self) -> bool {
        !self.cloud_history_valid
    }

    pub(super) fn mark_cloud_history_recorded(&mut self) {
        self.cloud_history_valid = true;
    }

    pub(super) fn invalidate_cloud_history(&mut self) {
        self.cloud_history_valid = false;
    }

    pub(super) fn terrain_ready(&self) -> bool {
        self.camera_initialized && self.terrain_history_valid
    }

    pub(super) fn available_mask(&self) -> u32 {
        let mut mask = 0;
        if self.terrain_ready() {
            mask |= DIRECT_SUN_SHADOW_SOURCE_TERRAIN;
        }
        if self.leaf_history_valid {
            mask |= DIRECT_SUN_SHADOW_SOURCE_LEAF;
        }
        if self.cloud_history_valid {
            mask |= DIRECT_SUN_SHADOW_SOURCE_CLOUD;
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_plan_owns_local_history_reset_policy() {
        let mut runtime = DirectSunShadowRuntime::default();
        assert!(runtime.camera_update_required(false));
        assert_eq!(runtime.available_mask(), 0);

        runtime.mark_camera_updated();
        let initial = runtime.plan_update(false);
        assert!(initial.reset_terrain_history());
        assert!(initial.reset_leaf_history());

        runtime.mark_terrain_history_recorded();
        runtime.mark_leaf_history_recorded();
        runtime.mark_cloud_history_recorded();
        assert!(!runtime.camera_update_required(false));
        assert_eq!(runtime.available_mask(), DIRECT_SUN_SHADOW_SOURCE_ALL);

        runtime.invalidate_local_histories();
        let invalidated = runtime.plan_update(false);
        assert!(invalidated.reset_terrain_history());
        assert!(invalidated.reset_leaf_history());
        assert_eq!(
            runtime.available_mask(),
            DIRECT_SUN_SHADOW_SOURCE_CLOUD,
            "local invalidation must preserve independent cloud-shadow history"
        );
    }

    #[test]
    fn dynamic_occluders_reset_only_the_terrain_filter_plan() {
        let mut runtime = DirectSunShadowRuntime::default();
        runtime.mark_camera_updated();
        runtime.mark_terrain_history_recorded();
        runtime.mark_leaf_history_recorded();

        let plan = runtime.plan_update(true);

        assert!(plan.reset_terrain_history());
        assert!(!plan.reset_leaf_history());
    }

    #[test]
    fn cloud_shadow_availability_follows_its_own_history() {
        let mut runtime = DirectSunShadowRuntime::default();
        assert!(runtime.cloud_history_reset_required());

        runtime.mark_cloud_history_recorded();
        assert!(!runtime.cloud_history_reset_required());
        assert_eq!(runtime.available_mask(), DIRECT_SUN_SHADOW_SOURCE_CLOUD);

        runtime.invalidate_cloud_history();
        assert!(runtime.cloud_history_reset_required());
        assert_eq!(runtime.available_mask(), 0);
    }
}
