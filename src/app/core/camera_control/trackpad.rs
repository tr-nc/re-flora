//! Route macOS precision gestures without synthesizing mouse buttons or UI wheel events.
use glam::Vec2;
use winit::event::{MouseScrollDelta, TouchPhase, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app::core) enum Gesture {
    Drag(Vec2, TouchPhase),
    Pinch(f64, TouchPhase),
}

impl Gesture {
    pub(in crate::app::core) fn from_window_event(
        event: &WindowEvent,
        macos: bool,
    ) -> Option<Self> {
        if !macos {
            return None;
        }
        match event {
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::PixelDelta(delta),
                phase,
                ..
            } => Some(Self::Drag(
                Vec2::new(delta.x as f32, delta.y as f32),
                *phase,
            )),
            WindowEvent::PinchGesture { delta, phase, .. } => Some(Self::Pinch(*delta, *phase)),
            _ => None,
        }
    }

    fn phase(self) -> TouchPhase {
        match self {
            Self::Drag(_, phase) | Self::Pinch(_, phase) => phase,
        }
    }

    fn is_finite(self) -> bool {
        match self {
            Self::Drag(delta, _) => delta.is_finite(),
            Self::Pinch(delta, _) => delta.is_finite(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Camera,
    Ui,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::core) enum Dispatch {
    Camera {
        began: bool,
    },
    /// Forward the original event to egui, but never let it fall through to the world.
    Ui,
    Ignore,
}

#[derive(Default)]
pub(in crate::app::core) struct TrackpadGestures {
    scroll: Option<Owner>,
    pinch: Option<Owner>,
}

impl TrackpadGestures {
    /// Cancel ownership as well as motion on focus loss/mode changes. A continuation
    /// must not start camera movement again until a fresh gesture begins.
    pub(in crate::app::core) fn cancel(&mut self) {
        self.scroll = Some(Owner::Blocked);
        self.pinch = Some(Owner::Blocked);
    }

    pub(in crate::app::core) fn cancel_scroll(&mut self) {
        self.scroll = Some(Owner::Blocked);
    }

    pub(in crate::app::core) fn route(
        &mut self,
        gesture: Gesture,
        ui_owned: bool,
        camera_available: bool,
    ) -> Dispatch {
        let slot = match gesture {
            Gesture::Drag(..) => &mut self.scroll,
            Gesture::Pinch(..) => &mut self.pinch,
        };
        let phase = gesture.phase();
        // macOS mayBegin and began both arrive as Started. Do not let the second
        // one steal a gesture that already belongs to a UI surface.
        let began =
            slot.is_none() || (phase == TouchPhase::Started && *slot == Some(Owner::Blocked));
        if began {
            *slot = Some(if ui_owned {
                Owner::Ui
            } else if camera_available {
                Owner::Camera
            } else {
                Owner::Blocked
            });
        } else if ui_owned {
            // Crossing into UI immediately restores normal UI handling. Moving back
            // out in the same gesture must not steal its remaining deltas for the camera.
            *slot = Some(Owner::Ui);
        } else if !camera_available && *slot == Some(Owner::Camera) {
            *slot = Some(Owner::Blocked);
        }
        let owner = slot.unwrap_or(Owner::Blocked);
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            *slot = None;
        }
        match owner {
            Owner::Ui => Dispatch::Ui,
            Owner::Camera if phase != TouchPhase::Cancelled && gesture.is_finite() => {
                Dispatch::Camera { began }
            }
            _ => Dispatch::Ignore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::{dpi::PhysicalPosition, event::DeviceId};

    fn scroll(delta: MouseScrollDelta, phase: TouchPhase) -> WindowEvent {
        WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta,
            phase,
        }
    }

    #[test]
    fn macos_pixel_scroll_retains_both_axes_and_other_inputs_stay_unchanged() {
        for (x, y) in [(8., 0.), (-8., 0.), (0., 14.), (0., -14.), (2., -14.)] {
            let event = scroll(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(x, y)),
                TouchPhase::Moved,
            );
            assert_eq!(
                Gesture::from_window_event(&event, true),
                Some(Gesture::Drag(
                    Vec2::new(x as f32, y as f32),
                    TouchPhase::Moved
                ))
            );
            assert_eq!(Gesture::from_window_event(&event, false), None);
        }
        assert_eq!(
            Gesture::from_window_event(
                &scroll(MouseScrollDelta::LineDelta(1., 2.), TouchPhase::Moved),
                true
            ),
            None
        );
        let pinch = WindowEvent::PinchGesture {
            device_id: DeviceId::dummy(),
            delta: -0.05,
            phase: TouchPhase::Started,
        };
        assert_eq!(
            Gesture::from_window_event(&pinch, true),
            Some(Gesture::Pinch(-0.05, TouchPhase::Started))
        );
    }

    #[test]
    fn ui_gestures_never_escape_to_camera_including_diagonals_and_pinch() {
        for make in [
            (|phase| Gesture::Drag(Vec2::new(8., -14.), phase)) as fn(TouchPhase) -> Gesture,
            |phase| Gesture::Pinch(0.04, phase),
        ] {
            let mut router = TrackpadGestures::default();
            assert_eq!(
                router.route(make(TouchPhase::Started), true, false),
                Dispatch::Ui
            );
            assert_eq!(
                router.route(make(TouchPhase::Started), false, true),
                Dispatch::Ui
            );
            assert_eq!(
                router.route(make(TouchPhase::Moved), false, true),
                Dispatch::Ui
            );
            assert_eq!(
                router.route(make(TouchPhase::Ended), false, true),
                Dispatch::Ui
            );
            assert_eq!(
                router.route(make(TouchPhase::Started), false, true),
                Dispatch::Camera { began: true }
            );
            assert_eq!(
                router.route(make(TouchPhase::Moved), true, false),
                Dispatch::Ui
            );
            assert_eq!(
                router.route(make(TouchPhase::Moved), false, true),
                Dispatch::Ui
            );
        }
    }

    #[test]
    fn precision_drag_matches_mouse_rotation_and_elevation_limits() {
        use super::super::CameraControlRuntime;
        use winit::event::MouseButton;
        for delta in [
            Vec2::new(8., 0.),
            Vec2::new(-8., 0.),
            Vec2::new(0., 14.),
            Vec2::new(0., -14.),
            Vec2::new(2., -14.),
            Vec2::new(20., 1000.),
        ] {
            let mut mouse = CameraControlRuntime::default();
            let mut touch = CameraControlRuntime::default();
            let start = Vec2::new(100., 80.);
            mouse.begin_orbit_rotation_drag(MouseButton::Right, Some(start));
            mouse.handle_orbit_drag(start + delta, true, -glam::Vec3::Z, 0.4);
            touch.begin_trackpad_rotation();
            touch.queue_orbit_rotation_delta(delta, 0.4);
            for _ in 0..20 {
                let expected = mouse.advance_orbit_motion(1. / 60., true);
                let actual = touch.advance_orbit_motion(1. / 60., true);
                assert_eq!(actual.rotation_delta, expected.rotation_delta);
                assert_eq!(actual.pan_delta, glam::Vec3::ZERO);
            }
            assert!(touch.orbit_drag.is_none(), "no synthetic held mouse button");
            assert!(
                touch
                    .advance_orbit_dolly(0.1, true, glam::Vec3::Z)
                    .is_none(),
                "scroll must not zoom"
            );
        }
    }

    #[test]
    fn trackpad_phase_restarts_preserve_queued_rotation() {
        use super::super::CameraControlRuntime;
        let mut camera = CameraControlRuntime::default();
        camera.begin_trackpad_rotation();
        camera.queue_orbit_rotation_delta(Vec2::new(10., 5.), 0.4);
        let before = camera.rotation_smoother.pending_delta();
        camera.begin_trackpad_rotation();
        assert_eq!(camera.rotation_smoother.pending_delta(), before);
    }

    #[test]
    fn pinch_targets_are_directional_proportional_and_independent_of_event_batching() {
        use super::super::CameraControlRuntime;
        let mut whole = CameraControlRuntime::default();
        let mut split = CameraControlRuntime::default();
        let position = whole.orbit_focus + glam::Vec3::Z * 2.;
        assert!(whole.pinch_dolly_lines(0.2, position) > 0.);
        assert!(whole.pinch_dolly_lines(-0.2, position) < 0.);
        assert_eq!(whole.pinch_dolly_lines(f64::NAN, position), 0.);
        assert_eq!(whole.pinch_dolly_lines(f64::INFINITY, position), 0.);
        let lines = whole.pinch_dolly_lines(0.2, position);
        whole.queue_mouse_wheel_dolly(lines);
        for _ in 0..20 {
            let lines = split.pinch_dolly_lines(0.01, position);
            split.queue_mouse_wheel_dolly(lines);
        }
        let a = whole.dolly_smoother.advance(0.1);
        let b = split.dolly_smoother.advance(0.1);
        assert!((a - b).abs() < 0.0001, "{a} != {b}");
    }

    #[test]
    fn ui_capture_or_cancellation_drops_pending_gesture_motion() {
        use super::super::CameraControlRuntime;
        let mut camera = CameraControlRuntime::default();
        camera.queue_orbit_rotation_delta(Vec2::new(30., 20.), 0.4);
        camera.queue_mouse_wheel_dolly(2.);
        camera.cancel_trackpad_motion(Gesture::Drag(Vec2::ZERO, TouchPhase::Cancelled));
        camera.cancel_trackpad_motion(Gesture::Pinch(0., TouchPhase::Cancelled));
        assert_eq!(
            camera.advance_orbit_motion(0.1, true).rotation_delta,
            glam::Vec3::ZERO
        );
        assert!(camera
            .advance_orbit_dolly(0.1, true, glam::Vec3::Z)
            .is_none());
    }

    #[test]
    fn cancellation_and_mode_changes_do_not_resurrect_camera_gestures() {
        let mut router = TrackpadGestures::default();
        let make = |phase| Gesture::Pinch(0.05, phase);
        assert_eq!(
            router.route(make(TouchPhase::Started), false, true),
            Dispatch::Camera { began: true }
        );
        router.cancel();
        assert_eq!(
            router.route(make(TouchPhase::Moved), false, true),
            Dispatch::Ignore
        );
        assert_eq!(
            router.route(make(TouchPhase::Started), false, true),
            Dispatch::Camera { began: true }
        );
        assert_eq!(
            router.route(make(TouchPhase::Cancelled), false, true),
            Dispatch::Ignore
        );
        assert_eq!(
            router.route(Gesture::Pinch(f64::NAN, TouchPhase::Started), false, true),
            Dispatch::Ignore
        );
        assert_eq!(
            router.route(
                Gesture::Drag(Vec2::splat(f32::INFINITY), TouchPhase::Started),
                false,
                true
            ),
            Dispatch::Ignore
        );
        assert_eq!(
            router.route(make(TouchPhase::Started), false, false),
            Dispatch::Ignore
        );
        assert_eq!(
            router.route(make(TouchPhase::Moved), false, true),
            Dispatch::Ignore
        );
    }
}
