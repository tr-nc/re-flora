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
pub(super) struct LightingModeAcceptanceFramePlan { timing: u32 }
pub(super) struct LightingModeAcceptanceRenderPlan { lighting: u32 }
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
let plan = LightingModeAcceptanceRuntime::frame_plan(&runtime);
let (timing, render) = LightingModeAcceptanceFramePlan::resolve_timing(plan, live_timing);
let lighting = LightingModeAcceptanceRenderPlan::resolve_lighting(render, live_lighting);
self.tracer.update_buffers(&time, &lighting, lights);
""",
        "src/tracer/mod.rs": """
pub struct Tracer;
impl Tracer {
    pub fn update_buffers(
        &mut self,
        lighting_frame: &ResolvedLightingFrameInputs,
    ) -> Result<()> {
        self.raster_lighting_mode = lighting_frame.raster_lighting_mode();
        crate::tracer::buffer_updater::BufferUpdater::update_gui_input(
            resources,
            lighting_frame,
        )
    }
}
""",
        "src/tracer/buffer_updater.rs": """
pub struct BufferUpdater;
impl BufferUpdater {
    pub fn update_gui_input(
        resources: &TracerResources,
        lighting_frame: &ResolvedLightingFrameInputs,
    ) -> Result<()> {
        resources.uniforms.gui_input.fill_uniform(&GuiInput {
            raster_flora_ddgi_lighting: lighting_frame.raster_lighting_mode().is_ddgi() as u32,
            path_tracing_reference: lighting_frame.path_tracing_reference() as u32,
            path_tracing_max_bounces: lighting_frame.path_tracing_max_bounces(),
            path_tracing_ambient_light: lighting_frame.path_tracing_ambient_light().to_array(),
        })
    }
}
""",
        "src/environment_lighting.rs": "pub struct EnvironmentLightingState;\n",
    }


class LightingModeAcceptanceSourceContractTests(unittest.TestCase):
    def test_current_recursive_source_tree_satisfies_contract(self) -> None:
        self.assertEqual(checker.audit(checker.read_sources(REPO_ROOT / "src")), [])

    def test_external_capsule_construction_and_qualified_plan_bypasses_are_rejected(self) -> None:
        mutations = (
            "let forged = ResolvedLightingFrameInputs { time_of_day: 0.0, sampling_serial: 1 };",
            "let ResolvedFrameTiming { visual_time_seconds, frame_delta_seconds } = timing;",
            "let f = LightingModeAcceptanceRuntime::frame_plan; f(&runtime);",
            "let f = LightingModeAcceptanceFramePlan::resolve_timing; f(plan, live);",
            "let f = LightingModeAcceptanceRenderPlan::resolve_lighting; f(render, live);",
            "let f = LightingModeAcceptanceRuntime::r#frame_plan; f(&runtime);",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/sibling.rs"] = mutation
                self.assertNotEqual(checker.audit(sources), [])

    def test_unrelated_receivers_may_reuse_plan_method_names(self) -> None:
        sources = baseline_sources()
        sources["src/app/sibling.rs"] = """
let a = scheduler.frame_plan();
let b = timer.resolve_timing(live);
let c = renderer.resolve_lighting(live);
let d = Scheduler::frame_plan(&scheduler);
let e = Timer::resolve_timing(timer, live);
let f = Renderer::resolve_lighting(renderer, live);
"""
        self.assertEqual(checker.audit(sources), [])

    def test_plan_resolution_requires_real_ufcs_calls_not_function_pointer_decoys(self) -> None:
        mutations = (
            (
                "LightingModeAcceptanceRuntime::frame_plan(&runtime)",
                "runtime.frame_plan()",
                "let _ = LightingModeAcceptanceRuntime::frame_plan;",
            ),
            (
                "LightingModeAcceptanceFramePlan::resolve_timing(plan, live_timing)",
                "plan.resolve_timing(live_timing)",
                "let _ = LightingModeAcceptanceFramePlan::resolve_timing;",
            ),
            (
                "LightingModeAcceptanceRenderPlan::resolve_lighting(render, live_lighting)",
                "render.resolve_lighting(live_lighting)",
                "let _ = LightingModeAcceptanceRenderPlan::resolve_lighting;",
            ),
        )
        for qualified_call, dot_call, decoy in mutations:
            with self.subTest(dot_call=dot_call):
                sources = baseline_sources()
                sources["src/app/core/mod.rs"] = (
                    sources["src/app/core/mod.rs"].replace(qualified_call, dot_call) + decoy
                )
                self.assertNotEqual(checker.audit(sources), [])

    def test_tracer_routes_its_direct_capsule_to_the_module_qualified_updater(self) -> None:
        mutations = (
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "dummy: fn(&ResolvedLightingFrameInputs), lighting_frame: bool,",
            ),
            (
                "crate::tracer::buffer_updater::BufferUpdater::update_gui_input(",
                "BufferUpdater::update_gui_input(",
            ),
            (
                "            lighting_frame,\n        )",
                "            forged_lighting_frame,\n        )",
            ),
            (
                "pub struct Tracer;",
                "use bypass::OtherUpdater as BufferUpdater;\npub struct Tracer;",
            ),
            (
                "crate::tracer::buffer_updater::BufferUpdater::update_gui_input(",
                """BypassUpdater::update_gui_input(resources, forged);
        crate::tracer::buffer_updater::BufferUpdater::update_gui_input(""",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/tracer/mod.rs"] = sources["src/tracer/mod.rs"].replace(before, after)
                if "OtherUpdater" in after:
                    sources["src/tracer/mod.rs"] = sources["src/tracer/mod.rs"].replace(
                        "crate::tracer::buffer_updater::BufferUpdater::update_gui_input(",
                        "BufferUpdater::update_gui_input(",
                    )
                self.assertNotEqual(checker.audit(sources), [])

    def test_tracer_state_mode_must_derive_from_the_same_capsule_sent_to_updater(self) -> None:
        mutations = (
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, forged_mode: RasterLightingMode,",
                "self.raster_lighting_mode = lighting_frame.raster_lighting_mode();",
                "self.raster_lighting_mode = forged_mode;",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "self.raster_lighting_mode = lighting_frame.raster_lighting_mode();",
                """let forged_mode = RasterLightingMode::Legacy;
        self.raster_lighting_mode = forged_mode;
        let decoy = lighting_frame.raster_lighting_mode();""",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "self.raster_lighting_mode = lighting_frame.raster_lighting_mode();",
                """(|lighting_frame: &ResolvedLightingFrameInputs| {
            self.raster_lighting_mode = lighting_frame.raster_lighting_mode();
        })(forged);""",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "self.raster_lighting_mode = lighting_frame.raster_lighting_mode();",
                """match forged {
            lighting_frame => {
                self.raster_lighting_mode = lighting_frame.raster_lighting_mode();
            }
        }""",
            ),
        )
        for signature_before, signature_after, state_before, state_after in mutations:
            with self.subTest(state_after=state_after):
                sources = baseline_sources()
                tracer = sources["src/tracer/mod.rs"].replace(
                    signature_before, signature_after
                )
                sources["src/tracer/mod.rs"] = tracer.replace(state_before, state_after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_updater_requires_one_direct_capsule_and_no_primitive_mode_parameter(self) -> None:
        mutations = (
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "dummy: fn(&ResolvedLightingFrameInputs), lighting_frame: bool,",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, dummy: RasterLightingMode,",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, dummy: Option<RasterLightingMode>,",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, path_tracing_reference: bool,",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, path_tracing_max_bounces: u32,",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, path_tracing_ambient_light: Vec3,",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/tracer/buffer_updater.rs"] = sources[
                    "src/tracer/buffer_updater.rs"
                ].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_each_lighting_uniform_value_must_use_its_inline_capsule_getter(self) -> None:
        mutations = (
            ("lighting_frame.raster_lighting_mode().is_ddgi() as u32", "forged_raster"),
            ("lighting_frame.path_tracing_reference() as u32", "forged_path"),
            ("lighting_frame.path_tracing_max_bounces()", "forged_bounces"),
            (
                "lighting_frame.path_tracing_ambient_light().to_array()",
                "forged_ambient",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/tracer/buffer_updater.rs"] = sources[
                    "src/tracer/buffer_updater.rs"
                ].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_updater_rejects_parameter_shadow_and_forged_or_decoy_uniform_values(self) -> None:
        mutations = (
            (
                "    ) -> Result<()> {",
                "    ) -> Result<()> { let lighting_frame = forged;",
            ),
            (
                "    ) -> Result<()> {",
                "    ) -> Result<()> { let resources = forged;",
            ),
            (
                "resources.uniforms.gui_input.fill_uniform(&GuiInput {",
                "resources.uniforms.gui_input.fill_uniform(&forged);\nother.fill_uniform(&GuiInput {",
            ),
            (
                "resources.uniforms.gui_input.fill_uniform(&GuiInput {",
                "other.fill_uniform(&decoy);\n        resources.uniforms.gui_input.fill_uniform(&GuiInput {",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/tracer/buffer_updater.rs"] = sources[
                    "src/tracer/buffer_updater.rs"
                ].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_shadowed_mode_and_capsule_decoy_cannot_authorize_a_forged_uniform(self) -> None:
        sources = baseline_sources()
        updater = sources["src/tracer/buffer_updater.rs"].replace(
            "lighting_frame.raster_lighting_mode().is_ddgi() as u32",
            "raster_lighting_mode.is_ddgi() as u32",
        )
        updater = updater.replace(
            "resources.uniforms.gui_input.fill_uniform(&GuiInput {",
            """let raster_lighting_mode = RasterLightingMode::Legacy;
        let decoy = lighting_frame.raster_lighting_mode().is_ddgi() as u32;
        resources.uniforms.gui_input.fill_uniform(&GuiInput {""",
        )
        sources["src/tracer/buffer_updater.rs"] = updater
        self.assertNotEqual(checker.audit(sources), [])

    def test_second_gui_input_writes_are_rejected_across_common_alias_forms(self) -> None:
        mutations = (
            "resources.uniforms.gui_input.fill_uniform(&value);",
            "let sink = &resources.uniforms.gui_input; sink.fill_uniform(&value);",
            "let receiver = &resources.uniforms.gui_input; let alias = receiver; alias.fill_uniform(&value);",
            "Buffer::fill_uniform(&resources.uniforms.gui_input, &value);",
            "(&resources.uniforms.gui_input).fill_uniform(&value);",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/sibling.rs"] = f"fn bypass(resources: &Resources) {{ {mutation} }}"
                self.assertNotEqual(checker.audit(sources), [])

    def test_owner_resolved_fields_reject_every_pub_visibility_form(self) -> None:
        for visibility in ("pub ", "pub(crate) ", "pub(super) ", "pub\n(crate)\n"):
            with self.subTest(visibility=visibility):
                sources = baseline_sources()
                sources["src/app/core/lighting_mode_acceptance.rs"] = sources[
                    "src/app/core/lighting_mode_acceptance.rs"
                ].replace("    time_of_day: f32,", f"    {visibility}time_of_day: f32,")
                self.assertNotEqual(checker.audit(sources), [])

    def test_comments_literals_and_raw_strings_are_not_contract_evidence(self) -> None:
        sources = baseline_sources()
        sources["src/app/decoys.rs"] = r'''
// LightingModeAcceptanceRuntime::frame_plan(&runtime)
/* ResolvedLightingFrameInputs { forged: true }
   /* Buffer::fill_uniform(&resources.uniforms.gui_input, &value); */
*/
const TEXT: &str = "ResolvedFrameTiming { pub(crate) visual_time_seconds: f32 }";
const RAW: &str = r###"LightingModeAcceptanceFramePlan::resolve_timing(plan, live)"###;
const BYTE_RAW: &[u8] = br##"resources.uniforms.gui_input.fill_uniform"##;
const C_RAW: &CStr = cr#"LightingModeAcceptanceRenderPlan::resolve_lighting"#;
const CHARS: [char; 3] = ['{', '}', '.'];
fn lifetime<'a>(value: &'a str) -> &'a str { value }
'''
        self.assertEqual(checker.audit(sources), [])


if __name__ == "__main__":
    unittest.main()
