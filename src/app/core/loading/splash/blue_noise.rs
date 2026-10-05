//! Seeded multi-class blue noise shared in design with the HTML atelier.
//! Only global counts are fixed; no row, column or small-block quotas.

pub(super) const SIZE: usize = 48;
const SIGMA: f64 = 1.5;
const STEPS: usize = 40_000;

pub(super) struct BlueNoise {
    classes: Vec<u8>,
}

impl BlueNoise {
    pub(super) fn new(seed: u64) -> Self {
        let n = SIZE * SIZE;
        let mut random = Random((seed ^ (seed >> 32)) as u32);
        let mut classes: Vec<u8> = (0..n).map(|i| (i % 3) as u8).collect();
        for i in (1..n).rev() {
            let j = (random.next() * (i + 1) as f64) as usize;
            classes.swap(i, j);
        }
        let mut offsets = Vec::new();
        for dy in -5i32..=5 {
            for dx in -5i32..=5 {
                if dx != 0 || dy != 0 {
                    let weight = (-((dx * dx + dy * dy) as f64) / (2.0 * SIGMA * SIGMA)).exp();
                    offsets.push((dx, dy, weight));
                }
            }
        }
        let count = offsets.len();
        let mut neighbors = vec![0u16; n * count];
        let mut potential = vec![0.0; n * 3];
        for i in 0..n {
            for (k, &(dx, dy, weight)) in offsets.iter().enumerate() {
                let x = (i as i32 % SIZE as i32 + dx).rem_euclid(SIZE as i32) as usize;
                let y = (i as i32 / SIZE as i32 + dy).rem_euclid(SIZE as i32) as usize;
                let neighbor = y * SIZE + x;
                neighbors[i * count + k] = neighbor as u16;
                potential[classes[i] as usize * n + neighbor] += weight;
            }
        }
        for step in 0..STEPS {
            let i = (random.next() * n as f64) as usize;
            let j = (random.next() * n as f64) as usize;
            let a = classes[i] as usize;
            let b = classes[j] as usize;
            if a == b {
                continue;
            }
            let dx = (i % SIZE).abs_diff(j % SIZE);
            let dy = (i / SIZE).abs_diff(j / SIZE);
            let dx = dx.min(SIZE - dx);
            let dy = dy.min(SIZE - dy);
            let pair = if dx <= 5 && dy <= 5 {
                (-((dx * dx + dy * dy) as f64) / (2.0 * SIGMA * SIGMA)).exp()
            } else {
                0.0
            };
            let delta = potential[b * n + i] + potential[a * n + j]
                - potential[a * n + i]
                - potential[b * n + j]
                - 2.0 * pair;
            let temperature = 0.12 * 0.04f64.powf(step as f64 / STEPS as f64);
            if delta > 0.0 && random.next() >= (-delta / temperature).exp() {
                continue;
            }
            for (k, &(_, _, weight)) in offsets.iter().enumerate() {
                let ni = neighbors[i * count + k] as usize;
                let nj = neighbors[j * count + k] as usize;
                potential[a * n + ni] -= weight;
                potential[b * n + ni] += weight;
                potential[b * n + nj] -= weight;
                potential[a * n + nj] += weight;
            }
            classes[i] = b as u8;
            classes[j] = a as u8;
        }
        Self { classes }
    }

    pub(super) fn class(&self, column: i32, row: i32) -> usize {
        self.classes
            [row.rem_euclid(SIZE as i32) as usize * SIZE + column.rem_euclid(SIZE as i32) as usize]
            as usize
    }
}

// Mulberry32, matching the demo's deterministic random stream.
struct Random(u32);
impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t));
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_field_is_stable_balanced_and_wraps_negative_coordinates() {
        let field = BlueNoise::new(42);
        let again = BlueNoise::new(42);
        assert_eq!(field.classes, again.classes);
        let mut counts = [0; 3];
        for &class in &field.classes {
            counts[class as usize] += 1;
        }
        assert_eq!(counts, [768; 3]);
        assert_eq!(field.class(-1, -1), field.class(47, 47));
        // Unlike the old layout, 3×3 blocks are not forced to equal counts.
        assert!((0..16).any(|block| {
            let mut counts = [0; 3];
            for y in 0..3 {
                for x in 0..3 {
                    counts[field.class(block * 3 + x, y)] += 1;
                }
            }
            counts != [3; 3]
        }));
        // A low-frequency projection is suppressed, rather than just shuffled.
        let mut power = 0.0;
        for class in 0..3 {
            for (kx, ky) in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                let mut re = 0.0;
                let mut im = 0.0;
                for y in 0..SIZE {
                    for x in 0..SIZE {
                        if field.class(x as i32, y as i32) == class {
                            let angle = std::f64::consts::TAU * (kx * x as f64 + ky * y as f64)
                                / SIZE as f64;
                            re += angle.cos();
                            im += angle.sin();
                        }
                    }
                }
                power += re * re + im * im;
            }
        }
        assert!(power / ((9 * SIZE * SIZE) as f64) < 0.1);
    }
}
