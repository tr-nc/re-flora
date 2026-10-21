//! Bounded latest-state, sample-domain valve rendering for resident PCM.
//! One control writer; the callback takes a coherent atomic snapshot once per block.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

/// Rendering horizon for an unrefreshed control (excludes buffered device latency).
pub const ENVELOPE_STALE_SECONDS: f64 = 0.2;

#[derive(Debug)]
struct Shared {
    epoch: Instant,
    sequence: AtomicU64,
    phase: AtomicU64,
    rate: AtomicU64,
    duty: AtomicU64,
    stamp: AtomicU64,
}

/// Latest authoritative phase, not a pulse-event queue. Clone only on control threads.
#[derive(Clone, Debug)]
pub struct SampleEnvelopeControl(Arc<Shared>);
impl Default for SampleEnvelopeControl {
    fn default() -> Self {
        Self(Arc::new(Shared {
            epoch: Instant::now(),
            sequence: AtomicU64::new(0),
            phase: AtomicU64::new(0),
            rate: AtomicU64::new(0),
            duty: AtomicU64::new(0),
            stamp: AtomicU64::new(0),
        }))
    }
}
impl SampleEnvelopeControl {
    /// Single-writer publication. Rate is normalized cycles per real second; zero silences.
    pub fn anchor(&self, phase: f64, rate: f64, duty: f64) {
        let s = &self.0;
        s.sequence.fetch_add(1, Ordering::SeqCst);
        s.phase.store(
            if phase.is_finite() {
                phase.rem_euclid(1.0)
            } else {
                0.0
            }
            .to_bits(),
            Ordering::SeqCst,
        );
        s.rate.store(
            if rate.is_finite() {
                rate.clamp(0.0, 240.0)
            } else {
                0.0
            }
            .to_bits(),
            Ordering::SeqCst,
        );
        s.duty.store(
            if duty.is_finite() {
                duty.clamp(0.0, 1.0)
            } else {
                0.0
            }
            .to_bits(),
            Ordering::SeqCst,
        );
        s.stamp
            .store(s.epoch.elapsed().as_secs_f64().to_bits(), Ordering::SeqCst);
        s.sequence.fetch_add(1, Ordering::SeqCst);
    }
    /// Includes queued, active and render-retirement ownership. Never reuse a slot early.
    pub fn has_render_owners(&self) -> bool {
        Arc::strong_count(&self.0) > 1
    }
}

/// Render state also usable by offline exporters; no allocation/locks in begin_block/next_gain.
#[derive(Debug)]
pub struct SampleEnvelopeRenderer {
    control: SampleEnvelopeControl,
    sequence: u64,
    phase: f64,
    rate: f64,
    duty: f64,
    age: f64,
    anchor_stamp: f64,
    gain: f32,
}
impl SampleEnvelopeRenderer {
    pub fn new(control: SampleEnvelopeControl) -> Self {
        Self {
            control,
            sequence: 0,
            phase: 0.0,
            rate: 0.0,
            duty: 0.0,
            age: 1.0,
            anchor_stamp: 0.0,
            gain: 0.0,
        }
    }
    pub fn begin_block(&mut self) {
        let s = &self.control.0;
        // Rendered sample age alone is insufficient after a device gap / full output ring.
        // Recheck wall age even when no new authoritative state was published.
        self.age = self
            .age
            .max((s.epoch.elapsed().as_secs_f64() - self.anchor_stamp).max(0.0));
        let a = s.sequence.load(Ordering::SeqCst);
        if a == self.sequence || a & 1 != 0 {
            return;
        }
        let phase = f64::from_bits(s.phase.load(Ordering::SeqCst));
        let rate = f64::from_bits(s.rate.load(Ordering::SeqCst));
        let duty = f64::from_bits(s.duty.load(Ordering::SeqCst));
        let stamp = f64::from_bits(s.stamp.load(Ordering::SeqCst));
        if s.sequence.load(Ordering::SeqCst) != a {
            return;
        }
        self.sequence = a;
        self.anchor_stamp = stamp;
        self.phase = phase;
        self.rate = rate;
        self.duty = duty;
        // Do not advance authoritative phase by control/queue latency. Like spatial poses,
        // this snapshot becomes the rendering anchor when consumed. Age still bounds silence.
        self.age = (s.epoch.elapsed().as_secs_f64() - stamp).max(0.0);
    }
    /// Valve equality is exact before the short edge shaping. At 100% there is no pulse cue.
    pub fn next_gain(&mut self, sample_rate: u32) -> f32 {
        let dt = 1.0 / sample_rate as f64;
        let mut target = 0.0;
        if self.rate > 0.0 && self.age < ENVELOPE_STALE_SECONDS && self.phase < self.duty {
            target = if self.duty >= 1.0 {
                0.72
            } else {
                let on_seconds = self.duty / self.rate;
                let edge = 0.001_f64.min(on_seconds * 0.25);
                let since = self.phase / self.rate;
                let until = (self.duty - self.phase) / self.rate;
                let shape = (since / edge).min(until / edge).clamp(0.0, 1.0);
                (shape * (0.72 + 0.28 * (-since / 0.012).exp())) as f32
            };
        }
        // Bounded de-click including stale controls, pause and phase discontinuities.
        // Narrow pulses use shortened interior edges above; the de-click tail is <= 1 ms.
        let step = (dt / 0.001) as f32;
        self.gain += (target - self.gain).clamp(-step, step);
        self.phase = (self.phase + self.rate * dt).rem_euclid(1.0);
        self.age += dt;
        self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "offline wall-clock/device-gap staleness regression; 220 ms sleep, no device"]
    fn unchanged_control_expires_across_a_render_or_device_gap() {
        let c = SampleEnvelopeControl::default();
        let mut r = SampleEnvelopeRenderer::new(c.clone());
        c.anchor(0.0, 20.0, 1.0);
        r.begin_block();
        for _ in 0..100 {
            r.next_gain(48000);
        }
        assert!(r.next_gain(48000) > 0.7);
        std::thread::sleep(std::time::Duration::from_millis(220));
        r.begin_block();
        for _ in 0..100 {
            r.next_gain(48000);
        }
        assert_eq!(r.next_gain(48000), 0.0);
    }

    #[test]
    fn continuous_has_no_pulse_and_edits_pause_resume_have_no_event_backlog() {
        let c = SampleEnvelopeControl::default();
        let mut r = SampleEnvelopeRenderer::new(c.clone());
        c.anchor(0.0, 20.0, 1.0);
        r.begin_block();
        for _ in 0..100 {
            r.next_gain(48000);
        }
        for _ in 0..4800 {
            assert_eq!(r.next_gain(48000), 0.72);
        }
        for (phase, rate, duty) in [(0.7, 5.0, 0.3), (0.9, 0.25, 0.1), (0.0, 0.0, 1.0)] {
            c.anchor(phase, rate, duty);
            r.begin_block();
            for _ in 0..100 {
                r.next_gain(48000);
            }
            assert_eq!(r.next_gain(48000), 0.0);
        }
        // An arbitrarily long pause cannot enqueue work. Resume opens only this valve.
        for _ in 0..48000 {
            assert_eq!(r.next_gain(48000), 0.0);
        }
        c.anchor(0.6, 20.0, 0.3);
        r.begin_block();
        for _ in 0..900 {
            assert_eq!(r.next_gain(48000), 0.0);
        }
        c.anchor(0.1, 20.0, 0.3);
        r.begin_block();
        for _ in 0..100 {
            r.next_gain(48000);
        }
        assert!(r.next_gain(48000) > 0.0);
    }

    #[test]
    fn production_envelope_tracks_low_cadence_valves_and_silences_stale_state() {
        for hz in [0.25, 5.0, 20.0] {
            for duty in [0.0, 0.01, 0.1, 0.3, 0.5, 0.9, 1.0] {
                for fps in [15, 30, 60, 120] {
                    let control = SampleEnvelopeControl::default();
                    let mut render = SampleEnvelopeRenderer::new(control.clone());
                    let mut energy = 0.0;
                    // Cover a complete 0.25 Hz period as well as fast valves.
                    for frame in 0..fps * 4 {
                        control.anchor((frame as f64 * hz / fps as f64) % 1.0, hz, duty);
                        render.begin_block();
                        for sample in 0..48000 / fps {
                            let gain = render.next_gain(48000);
                            assert!(gain.is_finite() && (0.0..=1.0).contains(&gain));
                            let phase =
                                ((frame as f64 / fps as f64 + sample as f64 / 48000.0) * hz) % 1.0;
                            if phase > duty + hz * 0.0011 && phase < 0.999 {
                                assert!(gain < 0.001);
                            }
                            energy += gain;
                        }
                    }
                    if duty == 0.0 {
                        assert_eq!(energy, 0.0);
                    } else {
                        assert!(energy > 0.0);
                    }
                    for _ in 0..10000 {
                        render.next_gain(48000);
                    }
                    assert_eq!(render.next_gain(48000), 0.0);
                    control.anchor(0.6, 20.0, 0.3);
                    render.begin_block();
                    assert_eq!(render.next_gain(48000), 0.0);
                    control.anchor(0.2, 0.0, 1.0);
                    render.begin_block();
                    for _ in 0..100 {
                        assert_eq!(render.next_gain(48000), 0.0);
                    }
                }
            }
        }
    }
}
