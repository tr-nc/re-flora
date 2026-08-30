from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
CHECKER_PATH = REPO_ROOT / "scripts" / "check_lighting_mode_acceptance_source_contract.py"
SPEC = importlib.util.spec_from_file_location("lighting_source_contract", CHECKER_PATH)
assert SPEC is not None and SPEC.loader is not None
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


def baseline_sources() -> dict[str, str]:
    return {
        "src/app/core/lighting_mode_acceptance.rs": """
pub(crate) enum RasterLightingMode { Ddgi, Legacy }
pub(super) struct ResolvedFrameTiming {
    visual_time_seconds: f32,
    frame_delta_seconds: f32,
}
pub(crate) struct ResolvedLightingFrameInputs {
    time_of_day: f32,
    sampling_serial: u32,
}
""",
        "src/app/core/mod.rs": """
let (timing, render) = runtime.frame_plan().resolve_timing(live_timing);
let lighting = render.resolve_lighting(live_lighting);
tracer.update_buffers(&time, &lighting, lights);
""",
        "src/tracer/mod.rs": """
pub fn update_buffers(
    &mut self,
    lighting_frame: &ResolvedLightingFrameInputs,
) -> Result<()> { todo!() }
""",
        "src/tracer/buffer_updater.rs": """
pub fn update_gui_input(
    resources: &TracerResources,
    raster_lighting_mode: RasterLightingMode,
    path_tracing_reference: bool,
) -> Result<()> { todo!() }
""",
        "src/environment_lighting.rs": "pub struct EnvironmentLightingState;\n",
    }


class LightingModeAcceptanceSourceContractTests(unittest.TestCase):
    def test_current_recursive_source_tree_satisfies_contract(self) -> None:
        self.assertEqual(checker.audit(checker.read_sources(REPO_ROOT / "src")), [])

    def test_fifth_module_construction_destructure_and_call_bypasses_are_rejected(self) -> None:
        mutations = (
            "let forged = ResolvedLightingFrameInputs { time_of_day: 0.0, sampling_serial: 1 };",
            "let ResolvedFrameTiming { visual_time_seconds, frame_delta_seconds } = timing;",
            "let second = runtime.frame_plan();",
            "let second = plan.resolve_timing(live);",
            "let second = render.resolve_lighting(live);",
            "let second = LightingModeAcceptanceRuntime::frame_plan(&runtime);",
            "let second = LightingModeAcceptanceFramePlan::resolve_timing(plan, live);",
            "let second = LightingModeAcceptanceRenderPlan::resolve_lighting(render, live);",
            "let f = LightingModeAcceptanceRuntime::frame_plan; f(&runtime);",
            "let f = LightingModeAcceptanceFramePlan::resolve_timing; f(plan, live);",
            "let f = LightingModeAcceptanceRenderPlan::resolve_lighting; f(render, live);",
            "pub fn update_buffers(&mut self, frame_serial_idx: u32, dither_strength_lsb: f32, path_tracing_reference: bool) -> Result<()> { todo!() }",
            "pub(crate) async unsafe fn update_buffers(\n&mut self,\nframe_serial_idx\n:\nu32,\ndither_strength_lsb: f32\n) -> Result<()> { todo!() }",
            "fn update_buffers(&mut self, path_tracing_reference: bool, path_tracing_max_bounces: u32) -> Result<()> { todo!() }",
            "fn update_buffers(&mut self, foo: bool) -> Result<()> { todo!() }",
            "pub(super) fn update_gui_input(raster_flora_ddgi_lighting: bool) -> Result<()> { todo!() }",
            "async fn update_gui_input(foo: bool) -> Result<()> { todo!() }",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/sibling.rs"] = mutation
                self.assertNotEqual(checker.audit(sources), [])

    def test_owner_resolved_fields_reject_every_pub_visibility_form(self) -> None:
        for visibility in ("pub ", "pub(crate) ", "pub(super) ", "pub\n(crate)\n"):
            with self.subTest(visibility=visibility):
                sources = baseline_sources()
                sources["src/app/core/lighting_mode_acceptance.rs"] = sources[
                    "src/app/core/lighting_mode_acceptance.rs"
                ].replace("    time_of_day: f32,", f"    {visibility}time_of_day: f32,")
                self.assertNotEqual(checker.audit(sources), [])

    def test_comments_strings_raw_strings_and_chars_are_not_source_contract_evidence(self) -> None:
        sources = baseline_sources()
        sources["src/app/decoys.rs"] = r'''
// runtime.frame_plan().resolve_timing(live).resolve_lighting(live)
/* ResolvedLightingFrameInputs { forged: true }
   /* nested LightingModeAcceptanceRuntime::frame_plan(&runtime); */
*/
const TEXT: &str = "ResolvedFrameTiming { pub(crate) visual_time_seconds: f32 }";
const RAW: &str = r###"pub(crate) async fn update_buffers(frame_serial_idx: u32)"###;
const CHARS: [char; 3] = ['{', '}', '.'];
'''
        self.assertEqual(checker.audit(sources), [])


if __name__ == "__main__":
    unittest.main()
