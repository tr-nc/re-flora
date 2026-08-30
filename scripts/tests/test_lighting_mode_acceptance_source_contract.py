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
pub struct Tracer;
impl Tracer {
    pub fn update_buffers(
        &mut self,
        lighting_frame: &ResolvedLightingFrameInputs,
    ) -> Result<()> {
        let raster_lighting_mode = lighting_frame.raster_lighting_mode();
        BufferUpdater::update_gui_input(resources, raster_lighting_mode)
    }
}
""",
        "src/tracer/buffer_updater.rs": """
pub struct BufferUpdater;
impl BufferUpdater {
    pub fn update_gui_input(
        resources: &TracerResources,
        raster_lighting_mode: RasterLightingMode,
        path_tracing_reference: bool,
    ) -> Result<()> {
        resources.uniforms.gui_input.fill_uniform(&GuiInput {
            raster_flora_ddgi_lighting: raster_lighting_mode.is_ddgi() as u32,
        })
    }
}
""",
        "src/environment_lighting.rs": "pub struct EnvironmentLightingState;\n",
    }


class LightingModeAcceptanceSourceContractTests(unittest.TestCase):
    def test_current_recursive_source_tree_satisfies_contract(self) -> None:
        self.assertEqual(checker.audit(checker.read_sources(REPO_ROOT / "src")), [])

    def test_fifth_module_construction_destructure_and_plan_bypasses_are_rejected(self) -> None:
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
            "let second = runtime.r#frame_plan();",
            "let f = LightingModeAcceptanceRuntime::r#frame_plan; f(&runtime);",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/sibling.rs"] = mutation
                self.assertNotEqual(checker.audit(sources), [])

    def test_canonical_typed_entries_require_inherent_impl_receiver_and_direct_types(self) -> None:
        mutations = (
            (
                "src/tracer/mod.rs",
                "impl Tracer {",
                "impl WrongTracer {",
            ),
            (
                "src/tracer/mod.rs",
                "&mut self,",
                "&self,",
            ),
            (
                "src/tracer/mod.rs",
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "dummy: fn(&ResolvedLightingFrameInputs), renamed: bool,",
            ),
            (
                "src/tracer/mod.rs",
                "BufferUpdater::update_gui_input(resources, raster_lighting_mode)",
                "BufferUpdater::update_gui_input(resources, RasterLightingMode::Legacy)",
            ),
            (
                "src/tracer/mod.rs",
                "fn update_buffers(",
                "fn r#update_buffers<T>(",
            ),
            (
                "src/tracer/mod.rs",
                "impl Tracer {",
                """impl Tracer {
    fn r#update_buffers<T>(&mut self, direct: &ResolvedLightingFrameInputs)
    where T: Copy { todo!() }
""",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "impl BufferUpdater {",
                "impl WrongBufferUpdater {",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "raster_lighting_mode: RasterLightingMode,",
                "&self, raster_lighting_mode: RasterLightingMode,",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "raster_lighting_mode: RasterLightingMode,",
                "RasterLightingMode: bool,",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "raster_lighting_mode: RasterLightingMode,",
                "dummy: Option<RasterLightingMode>, raster_lighting_mode: bool,",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "raster_lighting_mode.is_ddgi() as u32",
                "false as u32",
            ),
            (
                "src/tracer/buffer_updater.rs",
                "fn update_gui_input(",
                "fn r#update_gui_input<T>(",
            ),
        )
        for path, before, after in mutations:
            with self.subTest(path=path, after=after):
                sources = baseline_sources()
                sources[path] = sources[path].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_unrelated_update_helper_names_do_not_join_the_capability_contract(self) -> None:
        sources = baseline_sources()
        sources["src/builder/plain.rs"] = """
fn update_buffers<T>(value: T) where T: Copy { let _ = value; }
fn update_gui_input(foo: bool) { let _ = foo; }
"""
        self.assertEqual(checker.audit(sources), [])

    def test_gui_uniform_sink_is_unique_and_inside_the_typed_buffer_updater_entry(self) -> None:
        mutations = (
            (
                "src/tracer/buffer_updater.rs",
                "resources.uniforms.gui_input.fill_uniform",
                "resources.uniforms.other_input.fill_uniform",
            ),
            (
                "src/app/sibling.rs",
                "",
                "fn bypass(resources: &Resources) { resources.uniforms.gui_input.fill_uniform(&value); }",
            ),
            (
                "src/app/sibling.rs",
                "",
                "fn raw_bypass(resources: &Resources) { resources.uniforms.r#gui_input.r#fill_uniform(&value); }",
            ),
        )
        for path, before, after in mutations:
            with self.subTest(path=path, after=after):
                sources = baseline_sources()
                if before:
                    sources[path] = sources[path].replace(before, after)
                else:
                    sources[path] = after
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
const BYTE_RAW: &[u8] = br##"r#frame_plan ResolvedLightingFrameInputs { forged: true }"##;
const C_RAW: &CStr = cr#"r#update_gui_input resources.uniforms.gui_input.fill_uniform"#;
const CHARS: [char; 3] = ['{', '}', '.'];
fn lifetime<'a>(value: &'a str) -> &'a str { value }
'''
        self.assertEqual(checker.audit(sources), [])


if __name__ == "__main__":
    unittest.main()
