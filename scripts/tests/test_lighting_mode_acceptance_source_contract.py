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
pub(crate) struct ResolvedRasterLightingState {
    raster_lighting_mode: RasterLightingMode,
}
::static_assertions::assert_not_impl_any!(
    ResolvedRasterLightingState: ::core::marker::Copy, ::core::clone::Clone
);
pub(super) fn initial_raster_lighting_state() -> ResolvedRasterLightingState {
    ResolvedRasterLightingState { raster_lighting_mode: RasterLightingMode::Ddgi }
}
impl ResolvedLightingFrameInputs {
    pub(crate) fn raster_lighting_state(&self) -> ResolvedRasterLightingState {
        ResolvedRasterLightingState { raster_lighting_mode: self.raster_lighting_mode }
    }
}
impl ResolvedRasterLightingState {
    pub(crate) fn is_ddgi(&self) -> bool { self.raster_lighting_mode.is_ddgi() }
}
""",
        "src/app/core/mod.rs": """
impl App {
    fn new() {
        let tracer = Tracer::new(lighting_mode_acceptance::initial_raster_lighting_state());
        let plan = LightingModeAcceptanceRuntime::frame_plan(&runtime);
        let (timing, render) = LightingModeAcceptanceFramePlan::resolve_timing(plan, live_timing);
        let lighting = LightingModeAcceptanceRenderPlan::resolve_lighting(render, live_lighting);
        self.tracer.update_buffers(&time, &lighting, lights);
    }
}
""",
        "src/tracer/mod.rs": """
pub struct Tracer {
    raster_lighting_state: ResolvedRasterLightingState,
}
impl Tracer {
    pub fn new(raster_lighting_state: ResolvedRasterLightingState) -> Self {
        Self { raster_lighting_state }
    }

    pub fn update_buffers(
        &mut self,
        lighting_frame: &ResolvedLightingFrameInputs,
    ) -> Result<()> {
        self.raster_lighting_state = lighting_frame.raster_lighting_state();
        crate::tracer::buffer_updater::BufferUpdater::update_gui_input(
            resources,
            lighting_frame,
        )
    }

    fn raster_lighting_is_ddgi(&self) -> bool {
        self.raster_lighting_state.is_ddgi()
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

    def test_external_resolved_capsule_or_raster_state_construction_is_rejected(self) -> None:
        mutations = (
            "let forged = ResolvedLightingFrameInputs { time_of_day: 0.0, sampling_serial: 1 };",
            "let ResolvedFrameTiming { visual_time_seconds, frame_delta_seconds } = timing;",
            "let forged = ResolvedRasterLightingState { raster_lighting_mode: RasterLightingMode::Legacy };",
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/sibling.rs"] = mutation
                self.assertNotEqual(checker.audit(sources), [])

    def test_owner_cannot_add_a_second_resolved_raster_state_constructor(self) -> None:
        sources = baseline_sources()
        sources["src/app/core/lighting_mode_acceptance.rs"] += """
fn forge_state() -> ResolvedRasterLightingState {
    ResolvedRasterLightingState { raster_lighting_mode: RasterLightingMode::Legacy }
}
"""
        self.assertNotEqual(checker.audit(sources), [])

    def test_plan_call_syntax_and_unreachable_decoys_are_not_source_contract_claims(self) -> None:
        sources = baseline_sources()
        sources["src/app/core/mod.rs"] = """
impl App {
    fn new() {
        let tracer = Tracer::new(lighting_mode_acceptance::initial_raster_lighting_state());
        let plan = runtime.frame_plan();
        let (timing, render) = plan.resolve_timing(live_timing);
        let lighting = render.resolve_lighting(live_lighting);
        self.tracer.update_buffers(&time, &lighting, lights);
        if false {
            let _ = LightingModeAcceptanceRuntime::frame_plan;
            let _ = LightingModeAcceptanceFramePlan::resolve_timing;
            let _ = LightingModeAcceptanceRenderPlan::resolve_lighting;
        }
    }
}
"""
        sources["src/app/sibling.rs"] = """
let a = scheduler.frame_plan();
let b = timer.resolve_timing(live);
let c = renderer.resolve_lighting(live);
let d = Scheduler::frame_plan(&scheduler);
let e = Timer::resolve_timing(timer, live);
let f = Renderer::resolve_lighting(renderer, live);
"""
        self.assertEqual(checker.audit(sources), [])

    def test_tracer_requires_the_direct_capsule_and_opaque_raster_state_field(self) -> None:
        mutations = (
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "dummy: fn(&ResolvedLightingFrameInputs), lighting_frame: bool,",
            ),
            (
                "raster_lighting_state: ResolvedRasterLightingState,",
                "raster_lighting_state: Option<ResolvedRasterLightingState>,",
            ),
            (
                "raster_lighting_state: ResolvedRasterLightingState,",
                "raster_lighting_state: RasterLightingMode,",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/tracer/mod.rs"] = sources["src/tracer/mod.rs"].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_tracer_cannot_replace_opaque_state_with_a_forged_mode_or_alias(self) -> None:
        mutations = (
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, forged_mode: RasterLightingMode,",
                "self.raster_lighting_state = lighting_frame.raster_lighting_state();",
                "self.raster_lighting_state = forged_mode;",
            ),
            (
                "lighting_frame: &ResolvedLightingFrameInputs,",
                "lighting_frame: &ResolvedLightingFrameInputs, forged_mode: RasterLightingMode,",
                "self.raster_lighting_state = lighting_frame.raster_lighting_state();",
                """let forged_mode = RasterLightingMode::Legacy;
        let raster_lighting_state = forged_mode;
        self.raster_lighting_state = raster_lighting_state;""",
            ),
            (
                "",
                "",
                "self.raster_lighting_state = lighting_frame.raster_lighting_state();",
                "self.raster_lighting_state = ResolvedRasterLightingState { raster_lighting_mode: RasterLightingMode::Legacy };",
            ),
        )
        for signature_before, signature_after, state_before, state_after in mutations:
            with self.subTest(state_after=state_after):
                sources = baseline_sources()
                tracer = sources["src/tracer/mod.rs"]
                if signature_before:
                    tracer = tracer.replace(signature_before, signature_after)
                sources["src/tracer/mod.rs"] = tracer.replace(state_before, state_after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_owner_issues_the_only_initial_state_and_tracer_moves_it_into_new(self) -> None:
        mutations = (
            (
                "pub(super) fn initial_raster_lighting_state",
                "pub(crate) fn initial_raster_lighting_state",
            ),
            (
                "pub fn new(raster_lighting_state: ResolvedRasterLightingState)",
                "pub fn new(raster_lighting_state: Option<ResolvedRasterLightingState>)",
            ),
            (
                "Tracer::new(lighting_mode_acceptance::initial_raster_lighting_state())",
                "Tracer::new(forged_initial_state)",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                for path in (
                    "src/app/core/lighting_mode_acceptance.rs",
                    "src/app/core/mod.rs",
                    "src/tracer/mod.rs",
                ):
                    sources[path] = sources[path].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

        sources = baseline_sources()
        sources["src/tracer/mod.rs"] += (
            "fn bypass() { initial_raster_lighting_state(); }"
        )
        self.assertNotEqual(checker.audit(sources), [])

        sources = baseline_sources()
        sources["src/app/core/sibling.rs"] = (
            "fn bypass() { initial_raster_lighting_state(); }"
        )
        self.assertNotEqual(checker.audit(sources), [])

        sources = baseline_sources()
        sources["src/tracer/mod.rs"] += """
impl Tracer {
    fn accept_again(&mut self, state: ResolvedRasterLightingState) { consume(state); }
}
"""
        self.assertNotEqual(checker.audit(sources), [])

    def test_opaque_state_is_not_copy_or_clone_and_observation_borrows(self) -> None:
        mutations = (
            ("pub(crate) fn is_ddgi(&self)", "pub(crate) fn is_ddgi(self)"),
            (
                "pub(crate) fn raster_lighting_state(&self)",
                "pub(crate) fn raster_lighting_state(self)",
            ),
        )
        for before, after in mutations:
            with self.subTest(after=after):
                sources = baseline_sources()
                sources["src/app/core/lighting_mode_acceptance.rs"] = sources[
                    "src/app/core/lighting_mode_acceptance.rs"
                ].replace(before, after)
                self.assertNotEqual(checker.audit(sources), [])

    def test_rustc_non_copy_assertion_is_the_guarded_owner_artifact(self) -> None:
        sources = baseline_sources()
        sources["src/app/core/lighting_mode_acceptance.rs"] = sources[
            "src/app/core/lighting_mode_acceptance.rs"
        ].replace(
            "::static_assertions::assert_not_impl_any!(\n"
            "    ResolvedRasterLightingState: ::core::marker::Copy, ::core::clone::Clone\n"
            ");",
            "",
        )
        self.assertNotEqual(checker.audit(sources), [])

    def test_non_copy_assertion_paths_cannot_be_shadowed(self) -> None:
        self.assertEqual(checker.audit(baseline_sources()), [])
        absolute = (
            "::static_assertions::assert_not_impl_any!(\n"
            "    ResolvedRasterLightingState: ::core::marker::Copy, ::core::clone::Clone\n"
            ");"
        )
        mutations = (
            absolute.replace("::static_assertions", "static_assertions"),
            absolute.replace("::core::marker::Copy", "Copy"),
            absolute.replace("::core::clone::Clone", "Clone"),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                sources = baseline_sources()
                sources["src/app/core/lighting_mode_acceptance.rs"] = sources[
                    "src/app/core/lighting_mode_acceptance.rs"
                ].replace(absolute, mutation)
                self.assertNotEqual(checker.audit(sources), [])

        sources = baseline_sources()
        sources["src/app/core/lighting_mode_acceptance.rs"] = """
mod static_assertions {
    macro_rules! assert_not_impl_any { ($($tokens:tt)*) => {}; }
    pub(crate) use assert_not_impl_any;
}
""" + sources["src/app/core/lighting_mode_acceptance.rs"]
        self.assertEqual(checker.audit(sources), [])

    def test_non_copy_assertion_must_be_unconditional_module_root_item(self) -> None:
        self.assertEqual(checker.audit(baseline_sources()), [])
        assertion = (
            "::static_assertions::assert_not_impl_any!(\n"
            "    ResolvedRasterLightingState: ::core::marker::Copy, ::core::clone::Clone\n"
            ");"
        )
        replacements = (
            f"#[cfg(any())]\n{assertion}",
            f"#[cfg_attr(not(test), cfg(any()))]\n{assertion}",
            f"#[cfg(any())]\nmod disabled_proof {{ {assertion} }}",
            f"#[cfg(test)]\nmod tests {{ {assertion} }}",
            f"mod nested {{ {assertion} }}",
        )
        for replacement in replacements:
            with self.subTest(replacement=replacement):
                sources = baseline_sources()
                owner = sources["src/app/core/lighting_mode_acceptance.rs"]
                mutated = owner.replace(assertion, replacement)
                self.assertNotEqual(mutated, owner)
                sources["src/app/core/lighting_mode_acceptance.rs"] = mutated
                self.assertNotEqual(checker.audit(sources), [])

    def test_source_checker_does_not_guess_trait_alias_or_generic_semantics(self) -> None:
        sources = baseline_sources()
        sources["src/app/sibling.rs"] = """
use core::clone::Clone as Duplicate;
impl Duplicate for ResolvedRasterLightingState {
    fn clone(&self) -> Self { unreachable!() }
}
"""
        self.assertEqual(checker.audit(sources), [])

        sources = baseline_sources()
        sources["src/app/sibling.rs"] = """
struct Wrapper<T>(T);
impl Clone for Wrapper<ResolvedRasterLightingState> {
    fn clone(&self) -> Self { unreachable!() }
}
"""
        self.assertEqual(checker.audit(sources), [])

    def test_second_raster_state_write_anywhere_in_tracer_is_rejected(self) -> None:
        sources = baseline_sources()
        sources["src/tracer/mod.rs"] += """
impl Tracer {
    fn clear(&mut self) { self.raster_lighting_state = None; }
}
"""
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

    def test_updater_rejects_forged_or_decoy_uniform_sinks(self) -> None:
        mutations = (
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
        for field in ("time_of_day", "raster_lighting_mode"):
            for visibility in ("pub ", "pub(crate) ", "pub(super) ", "pub\n(crate)\n"):
                with self.subTest(field=field, visibility=visibility):
                    sources = baseline_sources()
                    owner = sources["src/app/core/lighting_mode_acceptance.rs"]
                    sources["src/app/core/lighting_mode_acceptance.rs"] = owner.replace(
                        f"    {field}:", f"    {visibility}{field}:", 1
                    )
                    self.assertNotEqual(checker.audit(sources), [])

    def test_module_root_field_visibility_cannot_hide_behind_nested_same_name(self) -> None:
        sources = baseline_sources()
        owner = sources["src/app/core/lighting_mode_acceptance.rs"]
        owner = """
mod decoy {
    struct ResolvedRasterLightingState { raster_lighting_mode: RasterLightingMode }
}
""" + owner.replace(
            "    raster_lighting_mode: RasterLightingMode,",
            "    pub(crate) raster_lighting_mode: RasterLightingMode,",
            1,
        )
        sources["src/app/core/lighting_mode_acceptance.rs"] = owner
        self.assertNotEqual(checker.audit(sources), [])

    def test_resolved_types_in_parameters_and_returns_are_not_construction(self) -> None:
        sources = baseline_sources()
        sources["src/app/sibling.rs"] = """
fn pass_state(
    value: ResolvedRasterLightingState,
) -> ResolvedRasterLightingState {
    value
}
fn pass_frame(value: &ResolvedLightingFrameInputs) -> &ResolvedLightingFrameInputs { value }
"""
        self.assertEqual(checker.audit(sources), [])

    def test_ordinary_bitwise_or_is_not_misclassified_as_parameter_shadowing(self) -> None:
        sources = baseline_sources()
        sources["src/tracer/mod.rs"] = sources["src/tracer/mod.rs"].replace(
            "self.raster_lighting_state = lighting_frame.raster_lighting_state();",
            """let combined_flags = left_flags | lighting_frame_bits | right_flags;
        consume(combined_flags);
        self.raster_lighting_state = lighting_frame.raster_lighting_state();""",
        )
        self.assertEqual(checker.audit(sources), [])

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
