use crate::generated::gpu_structs::{LightGpu, LocalLightInfo};
use crate::lighting::{
    LocalLightGpuPayload, LocalLightGpuSnapshot, LocalLightOverflow, LocalLightOverflowReason,
    ProviderId, LOCAL_LIGHT_GPU_CAPACITY,
};

pub(super) enum LocalLightLiveUpload<'a> {
    Info {
        info: &'a LocalLightInfo,
    },
    InfoAndLights {
        info: &'a LocalLightInfo,
        lights: &'a [LightGpu; LOCAL_LIGHT_GPU_CAPACITY],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocalLightLiveObservation<'a> {
    pub source_revision: Option<u64>,
    pub registry_revision: Option<u64>,
    pub live_revision: Option<u64>,
    pub count: u32,
    pub overflow: &'a [LocalLightOverflow],
}

impl LocalLightLiveObservation<'_> {
    pub(crate) const fn state(self) -> (Option<u64>, u32) {
        (self.source_revision, self.count)
    }
}

#[derive(Default)]
pub(super) struct LocalLightLivePublication {
    committed: Option<CommittedLocalLightPublication>,
}

struct CommittedLocalLightPublication {
    payload: LocalLightGpuPayload,
    overflow: Vec<LocalLightOverflow>,
}

impl LocalLightLivePublication {
    pub(super) fn publish<E>(
        &mut self,
        candidate: LocalLightGpuSnapshot,
        upload: impl FnOnce(LocalLightLiveUpload<'_>) -> Result<(), E>,
    ) -> Result<LocalLightGpuPayload, E> {
        let provisional_payload = candidate.payload();
        let selection_changed = self.committed.as_ref().map_or_else(
            || provisional_payload.count() > 0,
            |previous| !previous.payload.selection_eq(provisional_payload),
        );
        let previous_live_revision = self
            .committed
            .as_ref()
            .map_or(0, |previous| previous.payload.live_revision());
        let live_revision = if selection_changed {
            previous_live_revision.wrapping_add(1).max(1)
        } else {
            previous_live_revision
        };
        let candidate = candidate.with_live_revision(live_revision);
        let payload = candidate.payload();
        let metadata_changed = self.committed.as_ref().is_none_or(|previous| {
            previous.payload.source_revision() != payload.source_revision()
                || previous.payload.registry_revision() != payload.registry_revision()
        });

        if !selection_changed && !metadata_changed {
            return Ok(payload);
        }

        let upload_request = if selection_changed {
            LocalLightLiveUpload::InfoAndLights {
                info: &candidate.info,
                lights: &candidate.lights,
            }
        } else {
            LocalLightLiveUpload::Info {
                info: &candidate.info,
            }
        };
        upload(upload_request)?;

        log_publication(&candidate, selection_changed, live_revision);
        self.committed = Some(CommittedLocalLightPublication {
            payload,
            overflow: candidate.overflow,
        });
        Ok(payload)
    }

    pub(super) fn observation(&self) -> LocalLightLiveObservation<'_> {
        let Some(committed) = &self.committed else {
            return LocalLightLiveObservation {
                source_revision: None,
                registry_revision: None,
                live_revision: None,
                count: 0,
                overflow: &[],
            };
        };
        LocalLightLiveObservation {
            source_revision: Some(committed.payload.source_revision()),
            registry_revision: Some(committed.payload.registry_revision()),
            live_revision: Some(committed.payload.live_revision()),
            count: committed.payload.count(),
            overflow: &committed.overflow,
        }
    }
}

fn log_publication(candidate: &LocalLightGpuSnapshot, selection_changed: bool, live_revision: u64) {
    let payload = candidate.payload();
    log::info!(
        "[LOCAL_LIGHT][LIVE] source_revision={} registry_revision={} live_gpu_revision={} count={} capacity={} overflow_count={} selection_changed={} direct_upload=true",
        payload.source_revision(),
        payload.registry_revision(),
        live_revision,
        candidate.info.count,
        candidate.info.capacity,
        candidate.info.overflow_count,
        selection_changed,
    );
    let mut overflow_groups =
        std::collections::BTreeMap::<(ProviderId, LocalLightOverflowReason), usize>::new();
    for overflow in &candidate.overflow {
        *overflow_groups
            .entry((overflow.source.provider(), overflow.reason))
            .or_insert(0) += 1;
    }
    for ((provider, reason), count) in overflow_groups {
        log::info!(
            "[LOCAL_LIGHT][OVERFLOW] source_revision={} provider={} reason={} count={} explicit=true",
            payload.source_revision(),
            provider.get(),
            reason.label(),
            count,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::{LocalLight, LocalLightBudget, LocalLightRegistry, PointLight};
    use glam::Vec3;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum UploadKind {
        Info,
        InfoAndLights,
    }

    fn point(position_x: f32) -> LocalLight {
        LocalLight::Point(
            PointLight::new(Vec3::new(position_x, 0.5, 0.5), Vec3::ONE, 12.0, 0.02, 1.5)
                .expect("test point light must be valid"),
        )
    }

    fn candidate(lights: &LocalLightRegistry) -> LocalLightGpuSnapshot {
        LocalLightGpuSnapshot::from_authoritative(
            &lights.snapshot(),
            LocalLightBudget::point_lights(LOCAL_LIGHT_GPU_CAPACITY),
            0,
        )
    }

    fn upload_kind(upload: LocalLightLiveUpload<'_>) -> UploadKind {
        match upload {
            LocalLightLiveUpload::Info { .. } => UploadKind::Info,
            LocalLightLiveUpload::InfoAndLights { info, lights } => {
                assert!(info.count > 0 || lights.iter().all(|light| light.abi_version == 0));
                UploadKind::InfoAndLights
            }
        }
    }

    #[test]
    fn initial_empty_publication_uploads_only_metadata_without_advancing_live_revision() {
        let lights = LocalLightRegistry::default();
        let mut publication = LocalLightLivePublication::default();
        let mut uploads = Vec::new();

        let payload = publication
            .publish(candidate(&lights), |upload| {
                uploads.push(upload_kind(upload));
                Ok::<_, ()>(())
            })
            .unwrap();

        assert_eq!(uploads, [UploadKind::Info]);
        assert_eq!(payload.live_revision(), 0);
        assert_eq!(
            publication.observation(),
            LocalLightLiveObservation {
                source_revision: Some(0),
                registry_revision: Some(0),
                live_revision: Some(0),
                count: 0,
                overflow: &[],
            }
        );
    }

    #[test]
    fn selection_changes_upload_lights_and_unchanged_candidates_skip_upload() {
        let mut lights = LocalLightRegistry::default();
        let id = lights.add(point(0.25));
        let mut publication = LocalLightLivePublication::default();
        let mut uploads = Vec::new();

        publication
            .publish(candidate(&lights), |upload| {
                uploads.push(upload_kind(upload));
                Ok::<_, ()>(())
            })
            .unwrap();
        publication
            .publish(candidate(&lights), |_| -> Result<(), ()> {
                panic!("unchanged publication must not upload")
            })
            .unwrap();
        lights.remove(id).unwrap();
        publication
            .publish(candidate(&lights), |upload| {
                uploads.push(upload_kind(upload));
                Ok::<_, ()>(())
            })
            .unwrap();

        assert_eq!(
            uploads,
            [UploadKind::InfoAndLights, UploadKind::InfoAndLights]
        );
        assert_eq!(publication.observation().live_revision, Some(2));
        assert_eq!(publication.observation().count, 0);
    }

    #[test]
    fn overflow_only_change_uploads_metadata_and_preserves_live_revision() {
        let mut lights = LocalLightRegistry::default();
        for index in 0..LOCAL_LIGHT_GPU_CAPACITY {
            lights.add(point(index as f32 * 0.01));
        }
        let mut publication = LocalLightLivePublication::default();
        publication
            .publish(candidate(&lights), |_| Ok::<_, ()>(()))
            .unwrap();
        let live_revision = publication.observation().live_revision;

        lights.add(point(0.9));
        let mut upload = None;
        publication
            .publish(candidate(&lights), |request| {
                upload = Some(upload_kind(request));
                Ok::<_, ()>(())
            })
            .unwrap();

        assert_eq!(upload, Some(UploadKind::Info));
        assert_eq!(publication.observation().live_revision, live_revision);
        assert_eq!(publication.observation().overflow.len(), 1);
        assert_eq!(
            publication.observation().overflow[0].reason,
            LocalLightOverflowReason::Capacity
        );
    }

    #[test]
    fn failed_upload_does_not_commit_and_the_same_candidate_retries() {
        let mut lights = LocalLightRegistry::default();
        lights.add(point(0.25));
        let mut publication = LocalLightLivePublication::default();

        assert_eq!(
            publication.publish(candidate(&lights), |_| Err("upload failed")),
            Err("upload failed")
        );
        assert_eq!(publication.observation().source_revision, None);

        let mut retried = false;
        publication
            .publish(candidate(&lights), |request| {
                retried = upload_kind(request) == UploadKind::InfoAndLights;
                Ok::<_, ()>(())
            })
            .unwrap();
        assert!(retried);
        assert_eq!(publication.observation().live_revision, Some(1));
    }
}
