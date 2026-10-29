//! A bounded, reversible terminal connection to a published start section.
//! Never moves baked courses or grants an overlap/protection exemption.
use super::*;

impl PathStroke {
    /// Compile a local tangent-matched join. The caller still validates every
    /// course against terrain and coverage before adopting it on release.
    pub(in crate::app) fn close_loop(
        &self,
        last: PathSample,
        first: PathSample,
        fit: impl Fn(PathSample) -> bool,
    ) -> Option<Self> {
        if self.config.sampling != PathSampling::Courses {
            return None;
        }
        let entry = last.exit;
        let exit = first.entry;
        let chord = xz(exit.center - entry.center).length();
        // Do not shortcut a large unpublished pointer backlog across the garden.
        if !chord.is_finite()
            || chord < VOXEL_SIZE
            || f64::from(chord)
                > self
                    .mutable_support()
                    .max(f64::from(4. * self.config.width))
                    + f64::from(2. * self.config.width.max(self.config.pitch))
        {
            return None;
        }
        // Fit independent tangent handles: a corner return needs room outside
        // the corner, unlike an almost-tangent circular return. All candidates
        // obey the same material/coverage predicate; none bypass protection.
        for lead in [
            self.config.width.max(self.config.pitch),
            self.config.width * 0.5,
            0.,
        ] {
            for a in [1. / 3., 0.5, 0.75, 1.] {
                for b in [1. / 3., 0.5, 0.75, 1.] {
                    if let Some(join) =
                        self.join_curve(last, first, chord * a, chord * b, lead, &fit)
                    {
                        return Some(join);
                    }
                }
            }
        }
        None
    }

    fn join_curve(
        &self,
        last: PathSample,
        first: PathSample,
        start_handle: f32,
        end_handle: f32,
        lead: f32,
        fit: &impl Fn(PathSample) -> bool,
    ) -> Option<Self> {
        let entry = last.exit;
        let exit = first.entry;
        let forward = |s: CourseSection| Vec3::new(-s.across.z, 0., s.across.x);
        // Prefer a straight approach before the frozen start face. A matched
        // final tangent alone does not keep the outside edge off the first row.
        // Shorter approaches remain eligible only when all coverage checks pass.
        let approach = exit.center - forward(exit) * lead;
        let control = [
            entry.center,
            entry.center + forward(entry) * start_handle,
            approach - forward(exit) * end_handle,
            approach,
        ];
        let point = |t: f32| {
            let u = 1. - t;
            control[0] * u.powi(3)
                + control[1] * (3. * u * u * t)
                + control[2] * (3. * u * t * t)
                + control[3] * t.powi(3)
        };
        let tangent = |t: f32| {
            let u = 1. - t;
            let d = (control[1] - control[0]) * (3. * u * u)
                + (control[2] - control[1]) * (6. * u * t)
                + (control[3] - control[2]) * (3. * t * t);
            Vec3::new(d.x, 0., d.z).normalize_or_zero()
        };
        // Arc-length stations, independent of pointer event density. This is a
        // short cubic only; fixed bounded quadrature does not grow with history.
        const STEPS: usize = 128;
        let mut lengths = [0.; STEPS + 1];
        let mut previous = point(0.);
        for (i, length) in lengths.iter_mut().enumerate().skip(1) {
            let next = point(i as f32 / STEPS as f32);
            *length = xz(next - previous).length();
            previous = next;
        }
        for i in 1..=STEPS {
            lengths[i] += lengths[i - 1];
        }
        let curve_length = lengths[STEPS];
        let total = curve_length + lead;
        let section = |distance: f32| {
            if distance >= curve_length {
                return CourseSection::new(
                    approach + forward(exit) * (distance - curve_length),
                    forward(exit),
                    entry.continuity,
                );
            }
            let i = lengths.partition_point(|s| *s < distance).clamp(1, STEPS);
            let fraction =
                (distance - lengths[i - 1]) / (lengths[i] - lengths[i - 1]).max(f32::EPSILON);
            let t = ((i - 1) as f32 + fraction) / STEPS as f32;
            CourseSection::new(point(t), tangent(t), entry.continuity)
        };
        let next_index = last.index.checked_add(1)?;
        let preferred = (total / self.config.pitch).round().max(1.) as usize;
        // A closed masonry bond needs an even number of half-cells. Prefer the
        // nominal pitch, then try nearby complete partitions, never tiny filler.
        let mut counts: Vec<_> = (1..=MAX_QUEUED_ROWS)
            .filter(|n| (next_index as usize + n).is_multiple_of(2))
            .collect();
        counts.sort_by_key(|n| n.abs_diff(preferred));
        for count in counts
            .into_iter()
            .filter(|n| *n <= preferred.saturating_add(3))
        {
            let mut rows = VecDeque::new();
            let mut from = entry;
            for i in 0..count {
                let end = total * (i + 1) as f32 / count as f32;
                let to = if i + 1 == count { exit } else { section(end) };
                let middle = section(total * (i as f32 + 0.5) / count as f32);
                let row = PathSample {
                    center: middle.center,
                    forward: forward(middle),
                    distance: last.end_distance + total * (i as f32 + 0.5) / count as f32,
                    end_distance: last.end_distance + end,
                    index: next_index + i as u32,
                    entry: from,
                    exit: to,
                    closes_loop: i + 1 == count,
                    valid: course_fits(from, to, self.config.width),
                };
                let footprint = |r: PathSample| {
                    let half = self.config.width * 0.5;
                    [
                        r.entry.point(-half),
                        r.entry.point(half),
                        r.exit.point(half),
                        r.exit.point(-half),
                    ]
                };
                if !row.valid
                    || !fit(row)
                    || self.course_fit.as_ref().is_some_and(|fit| !fit(row))
                    || rows
                        .iter()
                        .any(|prior| overlaps_quads(footprint(*prior), footprint(row), -1e-6))
                {
                    break;
                }
                rows.push_back(row);
                from = to;
            }
            if rows.len() != count {
                continue;
            }
            let mut result = self.clone();
            result.rows = rows;
            let StrokeInput::Tiles(input) = &mut result.input else {
                unreachable!();
            };
            input.finished = true;
            input.dirty = false;
            return Some(result);
        }
        None
    }
}
