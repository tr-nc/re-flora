//! Partition only the mutable tail. Shared boundaries and actual terminal stations
//! survive publication; a legal inner lane is never omitted to rescue a bad course.
use super::*;

impl PathStroke {
    pub(super) fn adaptive_tail(
        &self,
        input: &TileInput,
        curve: &curve::RoundedPath,
    ) -> Vec<PathSample> {
        let pitch = f64::from(self.config.pitch);
        let start = input.next_distance;
        let count = (((curve.length - start + 1e-7) / pitch).floor() + 1.)
            .clamp(0., MAX_QUEUED_ROWS as f64) as usize;
        if count == 0 {
            return Vec::new();
        }
        let boundaries: Vec<_> = (0..=count)
            .map(|i| {
                if i == 0 {
                    if let Some(entry) = input.previous_exit {
                        return entry;
                    }
                }
                curve.section(start + (i as f64 - 0.5) * pitch)
            })
            .collect();
        let create = |i: usize, j: usize, index| {
            let a = start + (i as f64 - 0.5) * pitch;
            let b = start + (j as f64 - 0.5) * pitch;
            let middle = (a + b) * 0.5;
            let section = curve.section(middle);
            let entry = boundaries[i];
            let exit = boundaries[j];
            PathSample {
                center: section.center,
                forward: Vec3::new(-section.across.z, 0., section.across.x),
                distance: (input.distance_base + middle) as f32,
                end_distance: (input.distance_base + b) as f32,
                index,
                entry,
                exit,
                valid: entry.continuity == exit.continuity
                    && course_fits(entry, exit, self.config.width),
            }
        };
        let fit = self.course_fit.as_ref().expect("adaptive course policy");
        let mut choices = vec![[None; 2]; count + 1];
        choices[count] = [Some(count); 2];
        for i in (0..count).rev() {
            for parity in 0..2 {
                for j in i + 1..=count {
                    if choices[j][1 - parity].is_none() {
                        continue;
                    }
                    let row = create(i, j, parity as u32);
                    if row.valid
                        && (j == i + 1
                            || curve.follows_course(
                                start + (i as f64 - 0.5) * pitch,
                                start + (j as f64 - 0.5) * pitch,
                                row,
                                self.config.width,
                            ))
                        && fit(row)
                    {
                        choices[i][parity] = Some(j);
                        break;
                    }
                }
            }
        }
        let mut rows = Vec::new();
        let mut i = 0;
        let mut index = input.next_index;
        let complete = choices[0][index as usize % 2].is_some();
        while i < count {
            // No complete partition: preserve the real failure witness for the
            // adapter's ordinary rejection; never skip it or start beyond it.
            let j = if complete {
                choices[i][index as usize % 2].unwrap()
            } else {
                i + 1
            };
            rows.push(create(i, j, index));
            index = index.wrapping_add(1);
            i = j;
        }
        rows
    }

    pub(super) fn prepare_adaptive(&mut self) -> bool {
        let StrokeInput::Tiles(input) = &self.input else {
            return true;
        };
        if !input.dirty {
            return true;
        }
        let curve = self.curve(input);
        let limit = if input.finished {
            f64::INFINITY
        } else {
            curve.length - self.mutable_support()
        };
        let rows = self.adaptive_tail(input, &curve);
        let StrokeInput::Tiles(input) = &mut self.input else {
            unreachable!()
        };
        for row in rows {
            if f64::from(row.end_distance) - input.distance_base > limit + 1e-7 {
                break;
            }
            if self.rows.len() >= MAX_QUEUED_ROWS {
                return false;
            }
            input.previous_exit = Some(row.exit);
            input.next_distance = f64::from(row.end_distance) - input.distance_base
                + f64::from(self.config.pitch) * 0.5;
            input.next_index = row.index.wrapping_add(1);
            self.rows.push_back(row);
        }
        input.since_prepare = 0.;
        input.dirty = false;
        true
    }
}
