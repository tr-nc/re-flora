use petalsonic::{ProceduralAudioFactory, ProceduralAudioSource};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const TWO_PI: f32 = std::f32::consts::TAU;
const MAX_GRAINS: usize = 96;
const MAX_CREAKS: usize = 8;

#[derive(Debug)]
pub struct TreeRustleControl {
    wind_response_bits: AtomicU32,
    crackle_bits: AtomicU32,
}

impl Default for TreeRustleControl {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeRustleControl {
    pub fn new() -> Self {
        Self {
            wind_response_bits: AtomicU32::new(0.0f32.to_bits()),
            crackle_bits: AtomicU32::new(TreeRustlePreset::dense().crackle.to_bits()),
        }
    }

    pub fn set_wind_response(&self, wind_response: f32) {
        self.wind_response_bits
            .store(clamp(wind_response, 0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn wind_response(&self) -> f32 {
        clamp(
            f32::from_bits(self.wind_response_bits.load(Ordering::Relaxed)),
            0.0,
            1.0,
        )
    }

    #[allow(dead_code)]
    pub fn set_crackle(&self, crackle: f32) {
        self.crackle_bits
            .store(clamp(crackle, 0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    fn crackle(&self) -> f32 {
        clamp(
            f32::from_bits(self.crackle_bits.load(Ordering::Relaxed)),
            0.0,
            1.0,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TreeRustlePreset {
    pub leaf_density: f32,
    pub dryness: f32,
    pub branch: f32,
    pub air: f32,
    pub leaf_body: f32,
    pub crackle: f32,
    pub brightness: f32,
}

impl TreeRustlePreset {
    pub fn dense() -> Self {
        Self {
            leaf_density: 1.35,
            dryness: 0.20,
            branch: 0.06,
            air: 0.58,
            leaf_body: 1.14,
            crackle: 0.18,
            brightness: 0.42,
        }
    }
}

pub struct TreeRustleFactory {
    seed: u64,
    preset: TreeRustlePreset,
    control: Arc<TreeRustleControl>,
}

impl TreeRustleFactory {
    pub fn dense(seed: u64, control: Arc<TreeRustleControl>) -> Self {
        Self {
            seed,
            preset: TreeRustlePreset::dense(),
            control,
        }
    }
}

impl ProceduralAudioFactory for TreeRustleFactory {
    fn create(&self, sample_rate: u32) -> Box<dyn ProceduralAudioSource> {
        Box::new(TreeRustleVoice::new(
            sample_rate,
            self.seed,
            self.preset,
            self.control.clone(),
        ))
    }
}

struct TreeRustleVoice {
    sample_rate: f32,
    rng: FastRng,
    preset: TreeRustlePreset,
    control: Arc<TreeRustleControl>,
    wind: f32,
    leaf_activity: f32,
    air_lp: f32,
    air_lp2: f32,
    air_slow: f32,
    body_lp: f32,
    body_lp2: f32,
    body_slow: f32,
    leaf_hp: f32,
    leaf_lp: f32,
    leaf_lp2: f32,
    sheen_hp: f32,
    sheen_lp: f32,
    sheen_lp2: f32,
    air_hi_hp: f32,
    air_hi_lp: f32,
    air_hi_lp2: f32,
    leaf_flutter_phase: f32,
    grains: Vec<Grain>,
    creaks: Vec<BranchCreak>,
}

impl TreeRustleVoice {
    fn new(
        sample_rate: u32,
        seed: u64,
        preset: TreeRustlePreset,
        control: Arc<TreeRustleControl>,
    ) -> Self {
        let mut rng = FastRng::new(seed ^ 0x6a09_e667_f3bc_c909);
        Self {
            sample_rate: sample_rate as f32,
            rng,
            preset,
            control,
            wind: 0.0,
            leaf_activity: 0.0,
            air_lp: 0.0,
            air_lp2: 0.0,
            air_slow: 0.0,
            body_lp: 0.0,
            body_lp2: 0.0,
            body_slow: 0.0,
            leaf_hp: 0.0,
            leaf_lp: 0.0,
            leaf_lp2: 0.0,
            sheen_hp: 0.0,
            sheen_lp: 0.0,
            sheen_lp2: 0.0,
            air_hi_hp: 0.0,
            air_hi_lp: 0.0,
            air_hi_lp2: 0.0,
            leaf_flutter_phase: rng.uniform(0.0, TWO_PI),
            grains: Vec::with_capacity(MAX_GRAINS),
            creaks: Vec::with_capacity(MAX_CREAKS),
        }
    }

    fn render_sample(&mut self, coeffs: &BlockCoeffs) -> f32 {
        let preset = self.preset;
        let wind = self.wind;
        let bed_wind = coeffs.bed_wind;
        let leaf_activity = self.leaf_activity;
        let leaf_drive = 0.08 + 0.92 * leaf_activity;

        // Wide airy wind bed: low/mid filtered noise that stays stable while
        // leaf activity marks gust/contact rise.
        let raw = self.rng.bipolar();
        self.air_lp += (raw - self.air_lp) * coeffs.air_alpha;
        self.air_lp2 += (self.air_lp - self.air_lp2) * coeffs.air_alpha;
        self.air_slow += (self.air_lp2 - self.air_slow) * coeffs.air_slow_alpha;
        let air_amp = 0.120 * preset.air * (0.20 + coeffs.wind_lift);
        let mut out = (self.air_lp2 - 0.74 * self.air_slow) * air_amp;

        // Warm leaf body.
        let raw = self.rng.bipolar();
        self.body_lp += (raw - self.body_lp) * coeffs.body_alpha;
        self.body_lp2 += (self.body_lp - self.body_lp2) * coeffs.body_alpha;
        self.body_slow += (self.body_lp2 - self.body_slow) * coeffs.body_slow_alpha;
        let body_amp = 0.102
            * preset.leaf_body
            * preset.leaf_density
            * (0.24 + 0.76 * bed_wind.powf(1.28))
            * (1.15 - 0.28 * preset.dryness);
        out += (self.body_lp2 - 0.72 * self.body_slow) * body_amp;

        // Continuous leaf friction.
        let raw = self.rng.bipolar();
        self.leaf_hp += (raw - self.leaf_hp) * coeffs.leaf_hp_alpha;
        let high = raw - self.leaf_hp;
        self.leaf_lp += (high - self.leaf_lp) * coeffs.leaf_lp_alpha;
        self.leaf_lp2 += (self.leaf_lp - self.leaf_lp2) * coeffs.leaf_lp_alpha;
        let leaf_amp = 0.050
            * preset.leaf_density
            * leaf_drive.powf(1.75)
            * (0.42 + 0.40 * preset.dryness)
            * (0.54 + 0.46 * preset.brightness);
        out += self.leaf_lp2 * leaf_amp;

        // 3-6 kHz leaf-contact sheen.
        self.leaf_flutter_phase += TWO_PI * (5.0 + 4.0 * leaf_activity) / self.sample_rate;
        if self.leaf_flutter_phase > TWO_PI {
            self.leaf_flutter_phase -= TWO_PI;
        }
        let flutter_mod = 0.78 + 0.22 * (0.5 + 0.5 * self.leaf_flutter_phase.sin());
        let raw = self.rng.bipolar();
        self.sheen_hp += (raw - self.sheen_hp) * coeffs.sheen_hp_alpha;
        let high = raw - self.sheen_hp;
        self.sheen_lp += (high - self.sheen_lp) * coeffs.sheen_lp_alpha;
        self.sheen_lp2 += (self.sheen_lp - self.sheen_lp2) * coeffs.sheen_lp_alpha;
        let sheen_amp = 0.095
            * preset.leaf_density
            * leaf_drive.powf(1.60)
            * (0.32 + 0.68 * preset.brightness)
            * (0.54 + 0.34 * preset.dryness)
            * (0.74 + 0.58 * coeffs.crackle)
            * flutter_mod;
        out += self.sheen_lp2 * sheen_amp;

        // Restrained 8-12 kHz air.
        let raw = self.rng.bipolar();
        self.air_hi_hp += (raw - self.air_hi_hp) * coeffs.air_hi_hp_alpha;
        let high = raw - self.air_hi_hp;
        self.air_hi_lp += (high - self.air_hi_lp) * coeffs.air_hi_lp_alpha;
        self.air_hi_lp2 += (self.air_hi_lp - self.air_hi_lp2) * coeffs.air_hi_lp_alpha;
        let air_hi_amp = 0.0115
            * preset.leaf_density
            * leaf_activity.powf(2.00)
            * (0.20 + 0.80 * preset.brightness)
            * (0.62 + 0.30 * preset.dryness);
        out += self.air_hi_lp2 * air_hi_amp;

        self.maybe_spawn_grains(coeffs);
        self.maybe_spawn_creak(wind);
        out += self.render_grains();
        out += self.render_creaks();

        // Conservative runtime gain; source volume and PetalSonic headroom apply later.
        out * 0.72
    }

    fn maybe_spawn_grains(&mut self, coeffs: &BlockCoeffs) {
        if self.grains.len() >= MAX_GRAINS || coeffs.crackle <= 0.0001 {
            return;
        }

        let crackle_drive = coeffs.crackle.powf(1.15);
        let burst_rate = (0.14 + 30.0 * self.leaf_activity.powf(2.05))
            * self.preset.leaf_density
            * crackle_drive;
        if self.rng.next_f32() >= burst_rate / self.sample_rate {
            return;
        }

        let extra = (1.0 + 5.0 * self.leaf_activity * (0.40 + coeffs.crackle)) as usize;
        let mut cluster_count = 1 + self.rng.usize_below(1 + extra);
        if self.rng.next_f32() < (0.06 + 0.26 * self.leaf_activity) * coeffs.crackle {
            cluster_count += 1 + self.rng.usize_below(3);
        }
        cluster_count = cluster_count.min(MAX_GRAINS - self.grains.len());

        let max_window = 0.024f32.max(0.125 + 0.055 * self.leaf_activity - 0.045 * coeffs.crackle);
        let cluster_window =
            (self.sample_rate * self.rng.uniform(0.018, max_window)).max(1.0) as usize;
        for _ in 0..cluster_count {
            let delay = self.rng.usize_below(cluster_window.max(1));
            self.grains.push(Grain::new(
                &mut self.rng,
                self.sample_rate,
                self.leaf_activity,
                self.preset.dryness,
                self.preset.brightness,
                coeffs.crackle,
                delay,
            ));
        }
    }

    fn maybe_spawn_creak(&mut self, wind: f32) {
        if self.creaks.len() >= MAX_CREAKS {
            return;
        }

        let branch_rate = self.preset.branch * (wind - 0.42).max(0.0).powi(2) * 1.15;
        if self.rng.next_f32() < branch_rate / self.sample_rate {
            self.creaks.push(BranchCreak::new(
                &mut self.rng,
                self.sample_rate,
                wind,
                self.preset.branch,
            ));
        }
    }

    fn render_grains(&mut self) -> f32 {
        let mut out = 0.0;
        let mut write = 0;
        for read in 0..self.grains.len() {
            let mut grain = self.grains[read];
            let mut alive = true;

            if grain.delay > 0 {
                grain.delay -= 1;
            } else {
                grain.target *= grain.decay;
                grain.env += (grain.target - grain.env) * grain.attack_alpha;
                let raw = self.rng.bipolar();
                grain.hp_state += (raw - grain.hp_state) * grain.hp_alpha;
                let high = raw - grain.hp_state;
                grain.lp_state += (high - grain.lp_state) * grain.lp_alpha;
                grain.lp2_state += (grain.lp_state - grain.lp2_state) * grain.lp_alpha;
                out += grain.lp2_state * grain.env;
                alive = grain.env > 0.00005 || grain.target > 0.00005;
            }

            if alive {
                self.grains[write] = grain;
                write += 1;
            }
        }
        self.grains.truncate(write);
        out
    }

    fn render_creaks(&mut self) -> f32 {
        let mut out = 0.0;
        let mut write = 0;
        for read in 0..self.creaks.len() {
            let mut creak = self.creaks[read];
            creak.env *= creak.decay;
            creak.wobble_phase += TWO_PI * creak.wobble_frequency / self.sample_rate;
            let wobble = 1.0 + 0.11 * creak.wobble_phase.sin() + 0.035 * self.rng.bipolar();
            creak.phase += TWO_PI * creak.frequency * wobble / self.sample_rate;
            creak.noise_lp += (self.rng.bipolar() - creak.noise_lp) * 0.018;
            let tone = creak.phase.sin() + 0.35 * (creak.phase * 2.03 + 0.7).sin();
            out += (0.72 * tone + 0.28 * creak.noise_lp) * creak.env;

            if creak.env > 0.00003 {
                self.creaks[write] = creak;
                write += 1;
            }
        }
        self.creaks.truncate(write);
        out
    }
}

impl ProceduralAudioSource for TreeRustleVoice {
    fn render_mono(&mut self, out: &mut [f32]) {
        if out.is_empty() {
            return;
        }

        let target_wind = self.control.wind_response();
        let block_seconds = out.len() as f32 / self.sample_rate;
        let wind_cutoff = if target_wind >= self.wind { 3.2 } else { 1.8 };
        let wind_alpha = 1.0 - (-TWO_PI * wind_cutoff * block_seconds).exp();
        self.wind += (target_wind - self.wind) * wind_alpha.clamp(0.0, 1.0);
        self.wind = clamp(self.wind, 0.0, 1.0);

        let leaf_target = smoothstep(0.12, 0.92, self.wind);
        let leaf_cutoff = if leaf_target >= self.leaf_activity {
            0.55
        } else {
            0.28
        };
        let leaf_alpha = 1.0 - (-TWO_PI * leaf_cutoff * block_seconds).exp();
        self.leaf_activity += (leaf_target - self.leaf_activity) * leaf_alpha.clamp(0.0, 1.0);
        self.leaf_activity = clamp(self.leaf_activity, 0.0, 1.0);

        let coeffs = BlockCoeffs::new(
            self.sample_rate,
            self.wind,
            self.leaf_activity,
            self.preset,
            self.control.crackle(),
        );

        for sample in out {
            *sample = self.render_sample(&coeffs);
        }
    }

    fn reset(&mut self) {
        self.wind = self.control.wind_response();
        self.leaf_activity = smoothstep(0.12, 0.92, self.wind);
        self.air_lp = 0.0;
        self.air_lp2 = 0.0;
        self.air_slow = 0.0;
        self.body_lp = 0.0;
        self.body_lp2 = 0.0;
        self.body_slow = 0.0;
        self.leaf_hp = 0.0;
        self.leaf_lp = 0.0;
        self.leaf_lp2 = 0.0;
        self.sheen_hp = 0.0;
        self.sheen_lp = 0.0;
        self.sheen_lp2 = 0.0;
        self.air_hi_hp = 0.0;
        self.air_hi_lp = 0.0;
        self.air_hi_lp2 = 0.0;
        self.grains.clear();
        self.creaks.clear();
    }
}

#[derive(Clone, Copy)]
struct BlockCoeffs {
    bed_wind: f32,
    wind_lift: f32,
    crackle: f32,
    air_alpha: f32,
    air_slow_alpha: f32,
    body_alpha: f32,
    body_slow_alpha: f32,
    leaf_hp_alpha: f32,
    leaf_lp_alpha: f32,
    sheen_hp_alpha: f32,
    sheen_lp_alpha: f32,
    air_hi_hp_alpha: f32,
    air_hi_lp_alpha: f32,
}

impl BlockCoeffs {
    fn new(
        sample_rate: f32,
        wind: f32,
        leaf_activity: f32,
        preset: TreeRustlePreset,
        crackle: f32,
    ) -> Self {
        let bed_wind = clamp(0.08 + wind * 0.92, 0.0, 1.0);
        let wind_lift = bed_wind.powf(1.18);
        let crackle = clamp(crackle, 0.0, 1.0);

        Self {
            bed_wind,
            wind_lift,
            crackle,
            air_alpha: lowpass_alpha(460.0 + 720.0 * bed_wind, sample_rate),
            air_slow_alpha: lowpass_alpha(75.0 + 48.0 * bed_wind, sample_rate),
            body_alpha: lowpass_alpha(
                390.0 + 760.0 * bed_wind + 260.0 * preset.brightness,
                sample_rate,
            ),
            body_slow_alpha: lowpass_alpha(42.0 + 52.0 * bed_wind, sample_rate),
            leaf_hp_alpha: lowpass_alpha(
                210.0 + 330.0 * preset.dryness + 220.0 * preset.brightness + 180.0 * bed_wind,
                sample_rate,
            ),
            leaf_lp_alpha: lowpass_alpha(
                1450.0
                    + 1200.0 * preset.brightness
                    + 820.0 * preset.dryness
                    + 760.0 * leaf_activity,
                sample_rate,
            ),
            sheen_hp_alpha: lowpass_alpha(
                2200.0 + 460.0 * preset.dryness + 320.0 * preset.brightness + 300.0 * leaf_activity,
                sample_rate,
            ),
            sheen_lp_alpha: lowpass_alpha(
                4300.0
                    + 1050.0 * preset.brightness
                    + 540.0 * preset.dryness
                    + 760.0 * leaf_activity,
                sample_rate,
            ),
            air_hi_hp_alpha: lowpass_alpha(
                7200.0 + 900.0 * preset.brightness + 500.0 * preset.dryness,
                sample_rate,
            ),
            air_hi_lp_alpha: lowpass_alpha(
                10400.0 + 1200.0 * preset.brightness + 480.0 * leaf_activity,
                sample_rate,
            ),
        }
    }
}

#[derive(Clone, Copy)]
struct Grain {
    delay: usize,
    env: f32,
    target: f32,
    attack_alpha: f32,
    decay: f32,
    hp_state: f32,
    hp_alpha: f32,
    lp_state: f32,
    lp2_state: f32,
    lp_alpha: f32,
}

impl Grain {
    fn new(
        rng: &mut FastRng,
        sample_rate: f32,
        wind: f32,
        dryness: f32,
        brightness: f32,
        crackle: f32,
        delay: usize,
    ) -> Self {
        let duration = rng.uniform(0.038, 0.170 - 0.036 * dryness) * (1.12 - 0.40 * crackle);
        let duration = duration.max(0.018);
        let decay = (-1.0 / (duration * sample_rate)).exp();
        let attack_ms = (rng.uniform(4.5, 22.0) * (1.12 - 0.52 * crackle)).max(1.2);
        let attack_alpha = 1.0 - (-1.0 / (attack_ms * 0.001 * sample_rate)).exp();
        let hp_cutoff = rng.uniform(
            160.0 + 220.0 * dryness + 180.0 * brightness + 600.0 * crackle,
            620.0 + 520.0 * dryness + 760.0 * brightness + 1800.0 * crackle,
        );
        let lp_cutoff = rng.uniform(
            1400.0 + 520.0 * dryness + 700.0 * brightness + 1400.0 * crackle,
            3200.0 + 850.0 * dryness + 1500.0 * brightness + 2800.0 * crackle,
        );
        let amp = rng.uniform(0.007, 0.038)
            * (0.32 + 0.90 * wind)
            * (0.82 + 0.18 * dryness)
            * (0.22 + 1.20 * crackle);

        Self {
            delay,
            env: 0.0,
            target: amp,
            attack_alpha,
            decay,
            hp_state: 0.0,
            hp_alpha: lowpass_alpha(hp_cutoff, sample_rate),
            lp_state: 0.0,
            lp2_state: 0.0,
            lp_alpha: lowpass_alpha(lp_cutoff, sample_rate),
        }
    }
}

#[derive(Clone, Copy)]
struct BranchCreak {
    env: f32,
    decay: f32,
    phase: f32,
    wobble_phase: f32,
    frequency: f32,
    wobble_frequency: f32,
    noise_lp: f32,
}

impl BranchCreak {
    fn new(rng: &mut FastRng, sample_rate: f32, wind: f32, branch: f32) -> Self {
        let duration = rng.uniform(0.35, 1.25);
        Self {
            env: rng.uniform(0.035, 0.090) * wind * branch,
            decay: (-1.0 / (duration * sample_rate)).exp(),
            phase: rng.uniform(0.0, TWO_PI),
            wobble_phase: rng.uniform(0.0, TWO_PI),
            frequency: rng.uniform(75.0, 210.0),
            wobble_frequency: rng.uniform(1.1, 4.7),
            noise_lp: 0.0,
        }
    }
}

#[derive(Clone, Copy)]
struct FastRng {
    state: u64,
}

impl FastRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        ((x.wrapping_mul(0x2545_f491_4f6c_dd1d)) >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        const SCALE: f32 = 1.0 / ((1u32 << 24) as f32);
        ((self.next_u32() >> 8) as f32) * SCALE
    }

    fn bipolar(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }

    fn uniform(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    fn usize_below(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper
        }
    }
}

fn clamp(value: f32, low: f32, high: f32) -> f32 {
    value.min(high).max(low)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = clamp((value - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lowpass_alpha(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let cutoff_hz = clamp(cutoff_hz, 1.0, sample_rate * 0.45);
    1.0 - (-TWO_PI * cutoff_hz / sample_rate).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn highpass_rms(samples: &[f32], cutoff_hz: f32, sample_rate: f32) -> f32 {
        let alpha = lowpass_alpha(cutoff_hz, sample_rate);
        let mut lp = 0.0;
        let mut sum = 0.0;
        for sample in samples {
            lp += (*sample - lp) * alpha;
            let high = *sample - lp;
            sum += high * high;
        }
        (sum / samples.len() as f32).sqrt()
    }

    #[test]
    fn wind_control_changes_rendered_energy_and_brightness() {
        let control = Arc::new(TreeRustleControl::new());
        let mut voice =
            TreeRustleVoice::new(48_000, 42, TreeRustlePreset::dense(), control.clone());

        let mut quiet = vec![0.0; 48_000];
        control.set_wind_response(0.08);
        voice.render_mono(&mut quiet);

        let mut windy = vec![0.0; 48_000];
        control.set_wind_response(0.95);
        voice.render_mono(&mut windy);

        assert!(rms(&windy) > rms(&quiet) * 1.5);
        assert!(
            highpass_rms(&windy, 3_000.0, 48_000.0) > highpass_rms(&quiet, 3_000.0, 48_000.0) * 1.5
        );
        assert!(windy.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn controls_are_clamped() {
        let control = TreeRustleControl::new();
        control.set_wind_response(2.0);
        assert_eq!(control.wind_response(), 1.0);
        control.set_wind_response(-1.0);
        assert_eq!(control.wind_response(), 0.0);
        control.set_crackle(2.0);
        assert_eq!(control.crackle(), 1.0);
    }
}
