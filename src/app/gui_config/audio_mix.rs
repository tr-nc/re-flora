use super::saved_controls::SavedControls;

pub(super) fn draw(ui: &mut SavedControls<'_>) {
    ui.label("Live mixer");
    ui.small("1x = existing mix; 0x = silence. Save keeps all switches and scales.");
    macro_rules! channel {
        ($field:ident, $label:literal) => {
            ui.toggle(
                |s| &mut s.audio_mix.$field.enabled,
                true,
                false,
                concat!($label, " enabled"),
            );
            ui.slider(
                |s| &mut s.audio_mix.$field.scale,
                0.0..=8.0,
                concat!($label, " scale (x)"),
                0.05,
                false,
            );
        };
    }
    channel!(master, "Master");
    channel!(leaves, "Tree leaves");
    channel!(cicadas, "Cicadas");
    channel!(footsteps, "Footsteps / jump / landing");
    channel!(terrain, "Digging / terrain editing");
    channel!(interface, "Interface / item selection");
    ui.small("Master also respects M / --mute. Existing dB controls remain advanced input trims.");
}
