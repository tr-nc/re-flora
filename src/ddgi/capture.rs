use super::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildToken, DdgiFieldIdentity, DdgiFieldStage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiCaptureTarget {
    Iteration(u32),
    Converged,
    NonConverged,
    Published,
}

impl DdgiCaptureTarget {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "converged" => Some(Self::Converged),
            "non-converged" => Some(Self::NonConverged),
            "published" => Some(Self::Published),
            _ => value
                .strip_prefix('s')
                .and_then(|iteration| iteration.parse::<u32>().ok())
                .map(Self::Iteration),
        }
    }

    pub fn iteration(self) -> Option<u32> {
        match self {
            Self::Iteration(iteration) => Some(iteration),
            Self::Converged | Self::NonConverged | Self::Published => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Iteration(iteration) => format!("s{iteration}"),
            Self::Converged => "converged".to_owned(),
            Self::NonConverged => "non-converged".to_owned(),
            Self::Published => "published".to_owned(),
        }
    }

    pub fn matches(self, identity: DdgiFieldIdentity) -> bool {
        let field = identity.field();
        match self {
            Self::Iteration(iteration) => field.iteration() == iteration,
            Self::Converged => field.stage() == DdgiFieldStage::Converged,
            Self::NonConverged => field.stage() == DdgiFieldStage::NonConverged,
            Self::Published => true,
        }
    }

    pub fn matches_checkpoint(
        self,
        identity: DdgiFieldIdentity,
        publication: DdgiCapturePublication,
    ) -> bool {
        self.matches(identity)
            && (self != Self::Published || publication == DdgiCapturePublication::Published)
    }
}

impl Default for DdgiCaptureTarget {
    fn default() -> Self {
        Self::Iteration(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DdgiCapturePublication {
    Unpublished = 0,
    Published = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DdgiCaptureCheckpoint {
    pub build_token: DdgiBuildToken,
    pub field: DdgiFieldIdentity,
    pub validation: DdgiAtlasValidationStats,
    pub publication: DdgiCapturePublication,
    pub batch_order: DdgiBatchOrder,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddgi::{DdgiFieldKey, DdgiFieldStage};

    fn field(stage: DdgiFieldStage, iteration: u32) -> DdgiFieldIdentity {
        let source = (iteration > 0).then(|| {
            let source_stage = if iteration == 1 {
                DdgiFieldStage::SeedSky
            } else {
                DdgiFieldStage::Feedback
            };
            DdgiFieldKey::new(10, 3, 4, 32, source_stage, iteration - 1).unwrap()
        });
        DdgiFieldIdentity::new(
            DdgiFieldKey::new(11, 3, 4, 32, stage, iteration).unwrap(),
            source,
        )
        .unwrap()
    }

    #[test]
    fn parses_iteration_and_terminal_targets() {
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("s0"),
            Some(DdgiCaptureTarget::Iteration(0))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("s1"),
            Some(DdgiCaptureTarget::Iteration(1))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("s12"),
            Some(DdgiCaptureTarget::Iteration(12))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("converged"),
            Some(DdgiCaptureTarget::Converged)
        );
        assert_eq!(DdgiCaptureTarget::from_cli_value("sn"), None);
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("published"),
            Some(DdgiCaptureTarget::Published)
        );
    }

    #[test]
    fn iteration_target_accepts_terminal_classification_at_that_iteration() {
        let converged = field(DdgiFieldStage::Converged, 6);
        assert!(DdgiCaptureTarget::Iteration(6).matches(converged));
        assert!(DdgiCaptureTarget::Converged.matches(converged));
        assert!(!DdgiCaptureTarget::NonConverged.matches(converged));
        assert!(DdgiCaptureTarget::Published
            .matches_checkpoint(converged, DdgiCapturePublication::Published));
        assert!(!DdgiCaptureTarget::Published
            .matches_checkpoint(converged, DdgiCapturePublication::Unpublished));
    }
}
