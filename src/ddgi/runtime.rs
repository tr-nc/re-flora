use super::resources::{DdgiStatus, DdgiVolumeStatus};
use super::{
    DdgiAtlasValidationStats, DdgiBuildToken, DdgiFieldIdentity, DdgiRefreshState,
    DdgiResourceBytes, DdgiScheduledWork, DdgiScheduledWorkKind, DdgiTerrainRefresh,
    DdgiVolumeGrid, DdgiVolumeStage,
};

/// Semantic work identity exposed by the runtime without exposing scheduler operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdgiRuntimeTargetWork {
    kind: DdgiScheduledWorkKind,
    destination: DdgiFieldIdentity,
}

impl DdgiRuntimeTargetWork {
    pub fn kind(self) -> DdgiScheduledWorkKind {
        self.kind
    }

    pub fn destination(self) -> DdgiFieldIdentity {
        self.destination
    }
}

impl From<DdgiScheduledWork> for DdgiRuntimeTargetWork {
    fn from(work: DdgiScheduledWork) -> Self {
        Self {
            kind: work.kind(),
            destination: work.destination(),
        }
    }
}

/// Semantic evidence for one resident DDGI Volume.
///
/// Physical atlas layouts, ping-pong slots, ray-batch traversal, descriptors, and pipeline state
/// remain private to the DDGI implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiRuntimeVolumeStatus {
    pub build_token: Option<DdgiBuildToken>,
    pub grid: DdgiVolumeGrid,
    pub resource_bytes: DdgiResourceBytes,
    pub stage: DdgiVolumeStage,
    pub target_work: Option<DdgiRuntimeTargetWork>,
    pub complete_field: Option<DdgiFieldIdentity>,
    pub published_field: Option<DdgiFieldIdentity>,
    pub building_field: Option<DdgiFieldIdentity>,
    pub consecutive_below_threshold: u32,
    pub last_atlas_validation: Option<DdgiAtlasValidationStats>,
    pub global_sky_revision: u32,
    pub radiance_revision: Option<u32>,
    pub relocated_terrain_revision: Option<u32>,
    pub filtered_probe_count: u32,
}

impl DdgiRuntimeVolumeStatus {
    pub fn is_ready(self) -> bool {
        self.published_field.is_some()
    }
}

impl From<DdgiVolumeStatus> for DdgiRuntimeVolumeStatus {
    fn from(status: DdgiVolumeStatus) -> Self {
        Self {
            build_token: status.build_token,
            grid: status.grid,
            resource_bytes: status.resource_bytes,
            stage: status.stage,
            target_work: status.scheduled_work.map(Into::into),
            complete_field: status.complete_field,
            published_field: status.published_field,
            building_field: status.building_field,
            consecutive_below_threshold: status.consecutive_below_threshold,
            last_atlas_validation: status.last_atlas_validation,
            global_sky_revision: status.global_sky_revision,
            radiance_revision: status.radiance_revision,
            relocated_terrain_revision: status.relocated_terrain_revision,
            filtered_probe_count: status.filtered_probe_count,
        }
    }
}

/// Canonical observation of the DDGI Volume runtime lifecycle.
///
/// The variants make staging evidence available only while a staging volume exists. Callers can
/// observe logical identities, progress, and fail-closed state without learning atlas ownership or
/// descriptor publication ordering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiRuntimeStatus {
    state: DdgiRuntimeState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DdgiRuntimeState {
    Active {
        active: DdgiRuntimeVolumeStatus,
        coordinator: DdgiRefreshState,
        deferred_density_spacing_voxels: Option<u32>,
    },
    Staging {
        active: DdgiRuntimeVolumeStatus,
        staging: DdgiRuntimeVolumeStatus,
        coordinator: DdgiRefreshState,
        deferred_density_spacing_voxels: Option<u32>,
    },
}

impl DdgiRuntimeStatus {
    pub(crate) fn new(volumes: DdgiStatus, refresh: DdgiTerrainRefresh) -> Self {
        let active = volumes.active().into();
        let coordinator = refresh.state();
        let deferred_density_spacing_voxels = refresh.queued_density_spacing_voxels();
        match volumes.staging() {
            Some(staging) => {
                assert!(
                    staging.build_token.is_some(),
                    "DDGI staging status must have an immutable build token"
                );
                Self {
                    state: DdgiRuntimeState::Staging {
                        active,
                        staging: staging.into(),
                        coordinator,
                        deferred_density_spacing_voxels,
                    },
                }
            }
            None => Self {
                state: DdgiRuntimeState::Active {
                    active,
                    coordinator,
                    deferred_density_spacing_voxels,
                },
            },
        }
    }

    pub fn active(self) -> DdgiRuntimeVolumeStatus {
        match self.state {
            DdgiRuntimeState::Active { active, .. } | DdgiRuntimeState::Staging { active, .. } => {
                active
            }
        }
    }

    pub fn staging(self) -> Option<DdgiRuntimeVolumeStatus> {
        match self.state {
            DdgiRuntimeState::Active { .. } => None,
            DdgiRuntimeState::Staging { staging, .. } => Some(staging),
        }
    }

    pub fn builder(self) -> DdgiRuntimeVolumeStatus {
        self.staging().unwrap_or_else(|| self.active())
    }

    pub fn coordinator(self) -> DdgiRefreshState {
        match self.state {
            DdgiRuntimeState::Active { coordinator, .. }
            | DdgiRuntimeState::Staging { coordinator, .. } => coordinator,
        }
    }

    pub fn target_terrain_revision(self) -> Option<u32> {
        match self.coordinator() {
            DdgiRefreshState::AwaitingTerrain {
                latest_terrain_revision,
            }
            | DdgiRefreshState::BuildingTerrain {
                latest_terrain_revision,
                ..
            } => Some(latest_terrain_revision),
            DdgiRefreshState::Idle
            | DdgiRefreshState::DensityQueued { .. }
            | DdgiRefreshState::BuildingDensity { .. } => None,
        }
    }

    pub fn deferred_density_spacing_voxels(self) -> Option<u32> {
        match self.state {
            DdgiRuntimeState::Active {
                deferred_density_spacing_voxels,
                ..
            }
            | DdgiRuntimeState::Staging {
                deferred_density_spacing_voxels,
                ..
            } => deferred_density_spacing_voxels,
        }
    }

    pub fn full_domain_invalidation_is_fail_closed(self) -> bool {
        matches!(
            self.coordinator(),
            DdgiRefreshState::AwaitingTerrain { .. } | DdgiRefreshState::BuildingTerrain { .. }
        )
    }

    pub fn active_token_serial(self) -> Option<u64> {
        self.active().build_token.map(DdgiBuildToken::serial)
    }

    pub fn staging_token(self) -> Option<DdgiBuildToken> {
        self.staging().map(|staging| {
            staging
                .build_token
                .expect("DDGI staging status was validated at construction")
        })
    }

    pub fn active_line(self) -> String {
        let active = self.active();
        format!(
            "Active token {} · terrain {} · radiance {} · {} vox · {:?} · published {}",
            format_optional(self.active_token_serial()),
            format_optional(active.relocated_terrain_revision),
            format_optional(active.radiance_revision),
            active.grid.spacing_voxels(),
            active.stage,
            format_ddgi_field(active.published_field),
        )
    }

    pub fn builder_line(self) -> String {
        let Some(staging) = self.staging() else {
            return "Builder none".to_owned();
        };
        let token = self
            .staging_token()
            .expect("staging volume must expose its build token");
        format!(
            "Builder token {} · {:?} · terrain {} · radiance {} · {} vox · {}/{} filtered · {:?} · complete {} · building {} · published {}",
            token.serial(),
            token.kind(),
            format_optional(staging.relocated_terrain_revision),
            format_optional(staging.radiance_revision),
            staging.grid.spacing_voxels(),
            staging.filtered_probe_count,
            staging.grid.probe_count(),
            staging.stage,
            format_ddgi_field(staging.complete_field),
            format_ddgi_field(staging.building_field),
            format_ddgi_field(staging.published_field),
        )
    }

    pub fn coordinator_line(self) -> String {
        format!(
            "Target terrain {} · coordinator {:?} · density queued {}",
            format_optional(self.target_terrain_revision()),
            self.coordinator(),
            format_optional(self.deferred_density_spacing_voxels()),
        )
    }

    pub fn invalidation_line(self) -> &'static str {
        if self.full_domain_invalidation_is_fail_closed() {
            "Invalidation: full domain · fail-closed ON"
        } else {
            "Invalidation: none · fail-closed OFF"
        }
    }
}

fn format_optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "none".to_owned(), |value| value.to_string())
}

fn format_ddgi_field(value: Option<DdgiFieldIdentity>) -> String {
    value.map_or_else(
        || "none".to_owned(),
        |identity| {
            let field = identity.field();
            format!(
                "#{}/S{} {:?} <- {:?}",
                field.serial(),
                field.iteration(),
                field.stage(),
                identity.source().map(|source| source.serial()),
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{
        DdgiAtlasLayout, DdgiBuildKind, DdgiResourceBytes, DdgiTransportScheduler, DdgiVolumeGrid,
        DdgiVolumeStage,
    };
    use crate::geom::UAabb3;
    use glam::UVec3;

    fn field(geometry_revision: u32, radiance_revision: u32) -> super::DdgiFieldIdentity {
        let mut scheduler = DdgiTransportScheduler::new();
        scheduler.observe_radiance(radiance_revision);
        scheduler.request_geometry(geometry_revision, 16);
        scheduler.claim_next().unwrap().unwrap().destination()
    }

    fn volume_status(
        token: Option<DdgiBuildToken>,
        geometry_revision: u32,
        radiance_revision: u32,
        stage: DdgiVolumeStage,
    ) -> DdgiVolumeStatus {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let identity = field(geometry_revision, radiance_revision);
        DdgiVolumeStatus {
            build_token: token,
            grid,
            irradiance_layout: DdgiAtlasLayout::new(grid.probe_count(), 6).unwrap(),
            visibility_layout: DdgiAtlasLayout::new(grid.probe_count(), 14).unwrap(),
            resource_bytes: DdgiResourceBytes::for_grid(grid).unwrap(),
            stage,
            scheduled_work: None,
            complete_field: Some(identity),
            published_field: Some(identity),
            building_field: Some(identity),
            consecutive_below_threshold: 0,
            last_atlas_validation: None,
            global_sky_revision: radiance_revision,
            radiance_revision: Some(radiance_revision),
            relocated_terrain_revision: Some(geometry_revision),
            active_ray_batch: None,
            filtered_probe_count: 2048,
        }
    }

    #[test]
    fn staging_evidence_exists_only_in_the_staging_variant() {
        let grid = DdgiVolumeGrid::new(UVec3::splat(512), 16).unwrap();
        let active_token = DdgiBuildToken::for_test(7, 7, 16, DdgiBuildKind::Terrain);
        let active = volume_status(Some(active_token), 7, 3, DdgiVolumeStage::Ready);
        let active_only =
            DdgiRuntimeStatus::new(DdgiStatus::new(active, None), DdgiTerrainRefresh::default());
        assert!(matches!(active_only.state, DdgiRuntimeState::Active { .. }));
        assert!(active_only.staging().is_none());
        assert_eq!(
            active_only.active_line(),
            "Active token 7 · terrain 7 · radiance 3 · 16 vox · Ready · published #2/S1 SingleBounce <- Some(1)"
        );
        assert_eq!(active_only.builder_line(), "Builder none");
        assert_eq!(
            active_only.invalidation_line(),
            "Invalidation: none · fail-closed OFF"
        );

        let mut refresh = DdgiTerrainRefresh::default();
        refresh.request_density_rebuild(32);
        refresh.request(8, UAabb3::new(UVec3::splat(100), UVec3::splat(120)), grid);
        refresh.mark_terrain_published(8);
        let token = refresh.claim_next_build(16, 7).unwrap();
        let staging = volume_status(Some(token), 8, 4, DdgiVolumeStage::Rebuilding);
        let building = DdgiRuntimeStatus::new(DdgiStatus::new(active, Some(staging)), refresh);
        assert!(matches!(building.state, DdgiRuntimeState::Staging { .. }));
        assert_eq!(building.staging_token(), Some(token));
        assert!(building
            .builder_line()
            .starts_with("Builder token 1 · Terrain"));
        assert!(building.coordinator_line().contains("Target terrain 8"));
        assert!(building.coordinator_line().contains("density queued 32"));
        assert_eq!(
            building.invalidation_line(),
            "Invalidation: full domain · fail-closed ON"
        );
    }

    #[test]
    #[should_panic(expected = "DDGI staging status must have an immutable build token")]
    fn staging_without_a_build_token_fails_fast() {
        let active = volume_status(None, 7, 3, DdgiVolumeStage::Ready);
        let staging = volume_status(None, 8, 4, DdgiVolumeStage::Rebuilding);
        DdgiRuntimeStatus::new(
            DdgiStatus::new(active, Some(staging)),
            DdgiTerrainRefresh::default(),
        );
    }
}
