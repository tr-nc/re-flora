//! Fresh-world worn-earth approach and paired ordinary stone lanterns.
use super::{layout, App, TerrainBrushEdit, Vec2, Vec3};
use anyhow::Result;

impl App {
    pub(super) fn establish_temple_approach(&mut self, size: Vec2) -> Result<()> {
        let path = layout::approach_path(size);
        let ground = |p: Vec2, app: &Self| Vec3::new(p.x, app.query_terrain_height_cpu(p), p.y);
        // Ordinary grass edits leave natural soil intact and remain freely editable.
        // A short-grass verge softens the irregular, narrower bare-earth center.
        for span in path.windows(2) {
            let edit = TerrainBrushEdit {
                start: ground(span[0].center, self),
                end: ground(span[1].center, self),
                radius: (span[0].half_width + span[1].half_width) * 0.5,
            };
            self.apply_flora_trim_path(TerrainBrushEdit {
                radius: edit.radius + 0.03,
                ..edit
            })?;
            self.apply_surface_flora_removal(edit)?;
        }
        // Resolve all soil contacts before the first edit invalidates CPU terrain caches.
        let contacts: Vec<_> = layout::lantern_pairs(size)
            .into_iter()
            .flatten()
            .map(|p| ground(p, self))
            .collect();
        for position in &contacts {
            self.apply_surface_flora_removal(TerrainBrushEdit {
                start: *position,
                end: *position,
                radius: 0.038,
            })?;
            // Exactly the player tool's recipe, scale, clearance and publication path.
            self.place_stone_lantern(*position)?;
        }
        log::info!("[TEMPLE_APPROACH] path=curved_worn_earth samples={} variable_width=true short_grass_verge=true lantern_pairs={} paired_arc_length_stations=true ordinary_placement=true editable=true", path.len(), contacts.len() / 2);
        Ok(())
    }
}
