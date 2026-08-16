use super::{
    DdgiAtlasValidationStats, DdgiBatchOrder, DdgiBuildToken, DdgiFieldIdentity, DdgiFieldState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdgiCaptureTarget {
    Epoch(u32),
    Converged,
    Published,
}

impl DdgiCaptureTarget {
    pub fn from_cli_value(value: &str) -> Option<Self> {
        match value {
            "converged" => Some(Self::Converged),
            "published" => Some(Self::Published),
            _ => value
                .strip_prefix('e')
                .and_then(|epoch| epoch.parse::<u32>().ok())
                .map(Self::Epoch),
        }
    }

    pub fn update_epoch(self) -> Option<u32> {
        match self {
            Self::Epoch(epoch) => Some(epoch),
            Self::Converged | Self::Published => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Epoch(epoch) => format!("e{epoch}"),
            Self::Converged => "converged".to_owned(),
            Self::Published => "published".to_owned(),
        }
    }

    pub fn matches(self, identity: DdgiFieldIdentity) -> bool {
        let field = identity.field();
        match self {
            Self::Epoch(epoch) => field.update_epoch() == epoch,
            Self::Converged => field.state() == DdgiFieldState::Converged,
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
        Self::Epoch(0)
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
    use crate::ddgi::{DdgiFieldKey, DdgiFieldState};

    fn field(state: DdgiFieldState, epoch: u32) -> DdgiFieldIdentity {
        let source = (epoch > 0).then(|| {
            DdgiFieldKey::new(10, 3, 4, 32, DdgiFieldState::Converging, epoch - 1).unwrap()
        });
        DdgiFieldIdentity::new(
            DdgiFieldKey::new(11, 3, 4, 32, state, epoch).unwrap(),
            source,
        )
        .unwrap()
    }

    #[test]
    fn parses_epoch_and_terminal_targets() {
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("e0"),
            Some(DdgiCaptureTarget::Epoch(0))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("e1"),
            Some(DdgiCaptureTarget::Epoch(1))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("e12"),
            Some(DdgiCaptureTarget::Epoch(12))
        );
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("converged"),
            Some(DdgiCaptureTarget::Converged)
        );
        assert_eq!(DdgiCaptureTarget::from_cli_value("en"), None);
        assert_eq!(
            DdgiCaptureTarget::from_cli_value("published"),
            Some(DdgiCaptureTarget::Published)
        );
    }

    #[test]
    fn epoch_target_accepts_converged_classification_at_that_epoch() {
        let converged = field(DdgiFieldState::Converged, 6);
        assert!(DdgiCaptureTarget::Epoch(6).matches(converged));
        assert!(DdgiCaptureTarget::Converged.matches(converged));
        assert!(DdgiCaptureTarget::Published
            .matches_checkpoint(converged, DdgiCapturePublication::Published));
        assert!(!DdgiCaptureTarget::Published
            .matches_checkpoint(converged, DdgiCapturePublication::Unpublished));
    }
}
