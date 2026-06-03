#!/usr/bin/env python3
"""NiceGUI tuner for the procedural tree-rustle prototype.

Run from the tools directory:

    uv run python prototype_tree_rustle_gui.py

Then adjust sliders and press Render & Play. Rendered WAVs are written under
``target/audio-prototypes/gui`` so useful variants can be copied into a branch or
fed into the Rust implementation later.
"""

from __future__ import annotations

import argparse
import asyncio
import time
from pathlib import Path

from nicegui import app, ui

import prototype_tree_rustle as synth

OUTPUT_DIR = synth.ROOT / "target" / "audio-prototypes" / "gui"
OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
app.add_media_files("/rustle-audio", str(OUTPUT_DIR))

SLIDER_SPECS = {
    "base_wind": ("wind", 0.0, 1.0, 0.01, "overall wind strength"),
    "gustiness": ("gustiness", 0.0, 1.0, 0.01, "slow swell / release variation"),
    "leaf_density": ("leaf density", 0.0, 2.5, 0.01, "how many overlapping leaf events exist"),
    "leaf_body": ("leaf body", 0.0, 1.5, 0.01, "warm lower/mid rustle body; raise this if it sounds plasticky"),
    "crackle": ("crackle", 0.0, 1.0, 0.01, "short papery transients; lower this for less plastic bag"),
    "brightness": ("brightness", 0.0, 1.0, 0.01, "high-frequency cutoff / shine"),
    "dryness": ("dryness", 0.0, 1.0, 0.01, "dry leaves are sharper and more papery"),
    "air": ("air bed", 0.0, 1.5, 0.01, "wide whoosh underneath the leaves"),
    "branch": ("branch creak", 0.0, 1.0, 0.01, "rare low woody movement during stronger wind"),
}


class SliderRow:
    def __init__(self, key: str, value: float) -> None:
        label, minimum, maximum, step, hint = SLIDER_SPECS[key]
        self.key = key
        with ui.row().classes("w-full items-center gap-3"):
            ui.label(label).classes("w-28 text-right text-sm text-gray-700")
            self.slider = (
                ui.slider(min=minimum, max=maximum, step=step, value=value)
                .props("label-always")
                .classes("grow")
            )
            self.number = ui.number(min=minimum, max=maximum, step=step, value=value).classes("w-24")
            ui.label(hint).classes("w-72 text-xs text-gray-500")

        self.slider.bind_value(self.number)

    @property
    def value(self) -> float:
        return float(self.slider.value or 0.0)

    def set_value(self, value: float) -> None:
        self.slider.value = value
        self.slider.update()
        self.number.value = value
        self.number.update()


@ui.page("/")
def page() -> None:
    ui.page_title("Tree Rustle Tuner")

    with ui.column().classes("w-full max-w-6xl mx-auto p-4 gap-4"):
        ui.label("Procedural Tree Rustle Tuner").classes("text-2xl font-bold")
        ui.label(
            "If it sounds like cheap plastic bags: lower crackle/brightness/dryness, "
            "raise leaf body, and keep gustiness moderate."
        ).classes("text-sm text-gray-600")

        with ui.row().classes("w-full gap-4"):
            with ui.card().classes("grow"):
                ui.label("shape").classes("font-semibold")
                preset_select = ui.select(
                    sorted(synth.PRESETS),
                    label="preset",
                    value="dense",
                ).classes("w-64")

                sliders = {
                    key: SliderRow(key, getattr(synth.PRESETS["dense"], key)) for key in SLIDER_SPECS
                }

                def load_preset(name: str) -> None:
                    preset = synth.PRESETS[name]
                    for key, slider_row in sliders.items():
                        slider_row.set_value(float(getattr(preset, key)))
                    status.set_text(f"loaded preset: {name} — {preset.description}")

                preset_select.on_value_change(lambda event: load_preset(str(event.value)))

            with ui.card().classes("w-80"):
                ui.label("render").classes("font-semibold")
                duration_input = ui.number(
                    "duration seconds", value=8.0, min=0.25, max=30.0, step=0.25
                ).classes("w-full")
                seed_input = ui.number("seed", value=3, min=0, max=999_999, step=1).classes("w-full")
                sample_rate_select = ui.select(
                    [22_050, 44_100, 48_000], label="sample rate", value=48_000
                ).classes("w-full")
                peak_slider = (
                    ui.slider(min=0.10, max=1.0, step=0.01, value=0.82)
                    .props("label-always")
                    .classes("w-full")
                )
                ui.label("normalized peak").classes("text-xs text-gray-500")
                normalize_checkbox = ui.checkbox("normalize", value=True)
                autoplay_checkbox = ui.checkbox("try autoplay after render", value=True)
                render_button = ui.button("Render & Play", icon="play_arrow").classes("w-full")

                ui.separator()
                ui.label("browser player").classes("font-semibold")
                audio = ui.audio("", controls=True).classes("w-full")
                output_label = ui.label("no render yet").classes("text-xs text-gray-500")
                status = ui.label("ready").classes("text-sm")

        command_box = ui.textarea("matching CLI command").classes("w-full font-mono text-xs")

        def current_preset() -> synth.RustlePreset:
            base = synth.PRESETS[str(preset_select.value)]
            return synth.RustlePreset(
                base_wind=sliders["base_wind"].value,
                gustiness=sliders["gustiness"].value,
                leaf_density=sliders["leaf_density"].value,
                dryness=sliders["dryness"].value,
                branch=sliders["branch"].value,
                air=sliders["air"].value,
                leaf_body=sliders["leaf_body"].value,
                crackle=sliders["crackle"].value,
                brightness=sliders["brightness"].value,
                description=base.description,
            )

        def update_command(path: Path) -> None:
            preset = current_preset()
            command_box.value = (
                "uv run python prototype_tree_rustle.py "
                f"--duration {float(duration_input.value or 8.0):.2f} "
                f"--sample-rate {int(sample_rate_select.value or 48_000)} "
                f"--seed {int(seed_input.value or 0)} "
                f"--wind {preset.base_wind:.2f} "
                f"--gustiness {preset.gustiness:.2f} "
                f"--leaf-density {preset.leaf_density:.2f} "
                f"--leaf-body {preset.leaf_body:.2f} "
                f"--crackle {preset.crackle:.2f} "
                f"--brightness {preset.brightness:.2f} "
                f"--dryness {preset.dryness:.2f} "
                f"--air {preset.air:.2f} "
                f"--branch {preset.branch:.2f} "
                f"--peak {float(peak_slider.value or 0.82):.2f} "
                f"--out {path}"
            )
            if not normalize_checkbox.value:
                command_box.value += " --no-normalize"
            command_box.update()

        async def render() -> None:
            render_button.disable()
            status.set_text("rendering…")
            try:
                preset = current_preset()
                duration_seconds = max(0.25, float(duration_input.value or 8.0))
                sample_rate = int(sample_rate_select.value or 48_000)
                seed = int(seed_input.value or 0)
                peak_target = float(peak_slider.value or 0.82)
                output_path = OUTPUT_DIR / f"tree_rustle_{time.strftime('%Y%m%d_%H%M%S')}_{time.time_ns() % 1_000_000}.wav"

                left, right, controls = await asyncio.to_thread(
                    synth.render_rustle,
                    preset=preset,
                    duration_seconds=duration_seconds,
                    sample_rate=sample_rate,
                    seed=seed,
                )
                raw_peak, scale = await asyncio.to_thread(
                    synth.write_wav,
                    output_path,
                    left=left,
                    right=right,
                    sample_rate=sample_rate,
                    normalize=bool(normalize_checkbox.value),
                    peak_target=peak_target,
                )

                url = f"/rustle-audio/{output_path.name}?v={time.time_ns()}"
                audio.set_source(url)
                audio.update()
                if autoplay_checkbox.value:
                    audio.play()
                output_label.set_text(str(output_path))
                status.set_text(
                    f"raw_peak={raw_peak:.4f} scale={scale:.2f} "
                    f"wind_min={min(controls):.2f} wind_max={max(controls):.2f}"
                )
                update_command(output_path)
            except Exception as error:  # noqa: BLE001 - prototype GUI should surface any render failure
                status.set_text(f"error: {error}")
                raise
            finally:
                render_button.enable()

        render_button.on_click(render)
        update_command(OUTPUT_DIR / "tree_rustle.wav")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Open a NiceGUI procedural tree-rustle tuner.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8080)
    parser.add_argument("--no-open", action="store_true", help="do not open a browser automatically")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    ui.run(
        title="Tree Rustle Tuner",
        host=args.host,
        port=args.port,
        reload=False,
        show=not args.no_open,
    )
    return 0


if __name__ in {"__main__", "__mp_main__"}:
    raise SystemExit(main())
