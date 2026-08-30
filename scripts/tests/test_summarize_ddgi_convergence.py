from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
SUMMARIZER = SCRIPTS / "summarize_ddgi_convergence.py"


class SummarizeDdgiConvergenceTests(unittest.TestCase):
    def write_curve(
        self,
        run_dir: Path,
        *,
        absolute_threshold: float = 0.0025,
        relative_threshold: float = 0.02,
        relative_floor: float = 0.05,
        consecutive_epochs: int = 2,
        minimum_update_epochs: int = 8,
        terminal_reason: str = "Threshold",
        maximum_update_epochs: int = 128,
        include_policy: bool = True,
    ) -> None:
        stem = "sealed-spacing32-converged-forward"
        lines = []
        if include_policy:
            lines.append(
                "[DDGI] initialization requested terrain_revision=2 spacing_voxels=32 "
                "probes=4913 stage=RelocationPending "
                f"convergence_max_absolute_rgb_delta={absolute_threshold} "
                f"convergence_max_relative_rgb_delta={relative_threshold} "
                f"convergence_relative_floor={relative_floor} "
                f"convergence_consecutive_epochs={consecutive_epochs} "
                f"convergence_minimum_update_epochs={minimum_update_epochs} "
                f"convergence_maximum_update_epochs={maximum_update_epochs}"
            )
        samples = tuple((epoch, 0.5, 1.0, 0) for epoch in range(6)) + (
            (6, 0.002, 0.01, 1),
            (7, 0.001, 0.005, 2),
        )
        lines.append(
            "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated field_serial=1 geometry_revision=1 radiance_revision=1 "
            "spacing_voxels=32 state=Converging update_epoch=0 "
            "max_abs_rgb_delta=0.00000000 max_rel_rgb_delta=0.00000000 "
            "non_finite=0 negative_rgb_texels=0 valid_texels=64 "
            "scanned_stored_texels=100 abs_threshold=0.00250000 "
            f"rel_threshold={relative_threshold:.8f} consecutive_below=0/{consecutive_epochs}"
        )
        for field_serial, (epoch, absolute, relative, consecutive) in enumerate(samples, 2):
            lines.append(
                "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated "
                f"field_serial={field_serial} geometry_revision=2 radiance_revision=1 spacing_voxels=32 "
                f"state={'Converged' if epoch == 7 else 'Converging'} update_epoch={epoch} "
                f"max_abs_rgb_delta={absolute:.8f} "
                f"max_rel_rgb_delta={relative:.8f} "
                "non_finite=0 negative_rgb_texels=0 "
                "valid_texels=64 scanned_stored_texels=100 "
                f"abs_threshold={absolute_threshold:.8f} rel_threshold={relative_threshold:.8f} "
                f"consecutive_below={consecutive}/{consecutive_epochs}"
            )
        lines.append(
            "[DDGI_CONVERGENCE_EVIDENCE] terminal "
            "field_serial=9 geometry_revision=2 radiance_revision=1 spacing_voxels=32 "
            f"update_epoch=7 reason={terminal_reason}"
        )
        evidence = "\n".join(lines) + "\n"
        (run_dir / f"{stem}.console.log").write_text(evidence)
        (run_dir / f"{stem}.run.log").write_text(evidence)
        (run_dir / f"{stem}.analysis.json").write_text(
            json.dumps(
                {
                    "capture": {
                        "lifecycle_state": "converged",
                        "update_epoch": 7,
                        "spacing_voxels": 32,
                        "field_serial": 9,
                        "geometry_revision": 2,
                        "radiance_revision": 1,
                        "max_abs_delta": 0.001,
                        "max_rel_delta": 0.005,
                    },
                    "validation_failures": [],
                }
            )
        )

    def run_summarizer(
        self, run_dir: Path, output: Path, *, contract: Path | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                str(SUMMARIZER),
                "--run-dir",
                str(run_dir),
                "--output",
                str(output),
                "--cases",
                "sealed",
                "--spacings",
                "32",
                *(["--contract", str(contract)] if contract is not None else []),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_emits_qualified_temporal_curve(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir)

            result = self.run_summarizer(run_dir, output)
            report = json.loads(output.read_text())

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(report["qualified"])
        self.assertEqual(report["schema_version"], 2)
        self.assertEqual(report["matrix"]["curve_count"], 1)
        curve = report["curves"][0]
        self.assertEqual(curve["final_update_epoch"], 7)
        self.assertEqual(curve["terminal_reason"], "Threshold")
        self.assertEqual(len(curve["epochs"]), 8)
        self.assertEqual(report["policy"]["maximum_update_epoch"], 127)
        self.assertEqual(report["policy"]["relative_floor"], 0.05)

    def test_rejects_terminal_epoch_drift_inside_the_independent_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            contract = run_dir / "contract.toml"
            self.write_curve(run_dir)
            contract.write_text(
                (SCRIPTS.parent / "config" / "ddgi_convergence_acceptance.toml")
                .read_text()
                .replace("terminal_update_epoch = 127", "terminal_update_epoch = 126")
            )

            result = self.run_summarizer(run_dir, output, contract=contract)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("invalid DDGI convergence acceptance epoch contract", result.stderr)

    def test_rejects_runtime_epoch_count_drift_from_the_acceptance_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, maximum_update_epochs=64)

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("drifted from acceptance contract", result.stderr)

    def test_rejects_missing_duplicate_or_mismatched_terminal_evidence(self) -> None:
        mutations = (
            (
                "missing-validations",
                lambda text: "\n".join(
                    line
                    for line in text.splitlines()
                    if " full-atlas validated " not in line
                ),
            ),
            ("missing", lambda text: "\n".join(line for line in text.splitlines() if " terminal " not in line)),
            ("duplicate", lambda text: text + next(line for line in text.splitlines() if " terminal " in line) + "\n"),
            ("epoch", lambda text: text.replace("update_epoch=7 reason=Threshold", "update_epoch=6 reason=Threshold")),
            (
                "terminal-field",
                lambda text: text.replace(
                    "terminal field_serial=9", "terminal field_serial=8"
                ),
            ),
            (
                "curve-field",
                lambda text: text.replace(
                    "field_serial=4 geometry_revision=2",
                    "field_serial=40 geometry_revision=2",
                ),
            ),
        )
        for name, mutate in mutations:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                run_dir = Path(directory)
                output = run_dir / "summary.json"
                self.write_curve(run_dir)
                console = run_dir / "sealed-spacing32-converged-forward.console.log"
                console.write_text(mutate(console.read_text()))

                result = self.run_summarizer(run_dir, output)

                self.assertEqual(result.returncode, 1)
                self.assertFalse(output.exists())

    def test_console_only_evidence_cannot_qualify(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir)
            (run_dir / "sealed-spacing32-converged-forward.run.log").unlink()

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())

    def test_rejects_incomplete_or_changed_preserved_run_log_evidence(self) -> None:
        mutations = (
            ("truncated", lambda text: text.splitlines()[0] + "\n"),
            (
                "policy",
                lambda text: text.replace(
                    "convergence_relative_floor=0.05",
                    "convergence_relative_floor=0.06",
                ),
            ),
            (
                "epoch",
                lambda text: "\n".join(
                    line
                    for line in text.splitlines()
                    if not (
                        "full-atlas validated" in line and "update_epoch=4" in line
                    )
                )
                + "\n",
            ),
            (
                "terminal",
                lambda text: "\n".join(
                    line for line in text.splitlines() if " terminal " not in line
                )
                + "\n",
            ),
            (
                "duplicate-terminal",
                lambda text: text
                + next(line for line in text.splitlines() if " terminal " in line)
                + "\n",
            ),
        )
        for name, mutate in mutations:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                run_dir = Path(directory)
                output = run_dir / "summary.json"
                self.write_curve(run_dir)
                run_log = run_dir / "sealed-spacing32-converged-forward.run.log"
                run_log.write_text(mutate(run_log.read_text()))

                result = self.run_summarizer(run_dir, output)

                self.assertEqual(result.returncode, 1)
                self.assertFalse(output.exists())

    def test_rejects_capture_field_identity_that_differs_from_the_terminal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir)
            analysis = run_dir / "sealed-spacing32-converged-forward.analysis.json"
            payload = json.loads(analysis.read_text())
            payload["capture"]["field_serial"] = 8
            analysis.write_text(json.dumps(payload))

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("capture field serial mismatch", result.stderr)

    def test_rejects_a_foreign_later_curve_spliced_after_the_captured_field(self) -> None:
        def splice_foreign_curve(text: str) -> str:
            lines = text.splitlines()
            captured_final = next(
                line
                for line in lines
                if "full-atlas validated" in line and "field_serial=9" in line
            )
            captured_terminal = next(line for line in lines if " terminal " in line)
            lines.remove(captured_terminal)
            foreign_final = (
                captured_final.replace("field_serial=9", "field_serial=10")
                .replace("geometry_revision=2", "geometry_revision=3")
                .replace("radiance_revision=1", "radiance_revision=4")
            )
            foreign_terminal = (
                captured_terminal.replace("field_serial=9", "field_serial=10")
                .replace("geometry_revision=2", "geometry_revision=3")
                .replace("radiance_revision=1", "radiance_revision=4")
            )
            return "\n".join([*lines, foreign_final, foreign_terminal]) + "\n"

        for mutated_sources in (("console",), ("runlog",), ("console", "runlog")):
            with self.subTest(mutated_sources=mutated_sources), tempfile.TemporaryDirectory() as directory:
                run_dir = Path(directory)
                output = run_dir / "summary.json"
                self.write_curve(run_dir)
                paths = {
                    "console": run_dir
                    / "sealed-spacing32-converged-forward.console.log",
                    "runlog": run_dir / "sealed-spacing32-converged-forward.run.log",
                }
                for source in mutated_sources:
                    path = paths[source]
                    path.write_text(splice_foreign_curve(path.read_text()))

                result = self.run_summarizer(run_dir, output)

                self.assertEqual(result.returncode, 1)
                self.assertFalse(output.exists())
                if len(mutated_sources) == 2:
                    self.assertIn(
                        "terminal field_serial differs from the captured curve final record",
                        result.stderr,
                    )

    def test_rejects_same_line_duplicate_evidence_in_each_process_stream(self) -> None:
        def duplicate_evidence_line(text: str, marker: str) -> str:
            lines = text.splitlines()
            index = next(index for index, line in enumerate(lines) if marker in line)
            lines[index] = f"{lines[index]} {lines[index]}"
            return "\n".join(lines) + "\n"

        for marker in ("full-atlas validated", " terminal "):
            for mutated_sources in (("console",), ("runlog",), ("console", "runlog")):
                with (
                    self.subTest(marker=marker, mutated_sources=mutated_sources),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    run_dir = Path(directory)
                    output = run_dir / "summary.json"
                    self.write_curve(run_dir)
                    paths = {
                        "console": run_dir
                        / "sealed-spacing32-converged-forward.console.log",
                        "runlog": run_dir
                        / "sealed-spacing32-converged-forward.run.log",
                    }
                    for source in mutated_sources:
                        path = paths[source]
                        path.write_text(duplicate_evidence_line(path.read_text(), marker))

                    result = self.run_summarizer(run_dir, output)

                    self.assertEqual(result.returncode, 1)
                    self.assertFalse(output.exists())
                    self.assertIn(
                        "exactly one DDGI convergence evidence marker",
                        result.stderr,
                    )

    def test_rejects_junk_between_marker_and_payload_in_each_process_stream(self) -> None:
        def insert_junk_after_marker(text: str, marker: str) -> str:
            lines = text.splitlines()
            index = next(index for index, line in enumerate(lines) if marker in line)
            lines[index] = lines[index].replace(marker, f"{marker} malformed-junk", 1)
            return "\n".join(lines) + "\n"

        for marker in (
            "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated",
            "[DDGI_CONVERGENCE_EVIDENCE] terminal",
        ):
            for mutated_sources in (("console",), ("runlog",), ("console", "runlog")):
                with (
                    self.subTest(marker=marker, mutated_sources=mutated_sources),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    run_dir = Path(directory)
                    output = run_dir / "summary.json"
                    self.write_curve(run_dir)
                    paths = {
                        "console": run_dir
                        / "sealed-spacing32-converged-forward.console.log",
                        "runlog": run_dir
                        / "sealed-spacing32-converged-forward.run.log",
                    }
                    for source in mutated_sources:
                        path = paths[source]
                        path.write_text(insert_junk_after_marker(path.read_text(), marker))

                    result = self.run_summarizer(run_dir, output)

                    self.assertEqual(result.returncode, 1)
                    self.assertFalse(output.exists())
                    self.assertIn("malformed", result.stderr)

    def test_rejects_extra_validation_fields_in_each_process_stream(self) -> None:
        def mutate_validation(text: str, replacement: str) -> str:
            return text.replace("update_epoch=4 ", f"update_epoch=4 {replacement} ", 1)

        mutations = (
            ("epoch-junk", "unexpected=value"),
            (
                "duplicate-identity",
                "field_serial=5 geometry_revision=2 radiance_revision=1 spacing_voxels=32",
            ),
            (
                "fake-stats",
                "max_abs_rgb_delta=0.00000000 max_rel_rgb_delta=0.00000000",
            ),
        )
        for name, replacement in mutations:
            for mutated_sources in (("console",), ("runlog",), ("console", "runlog")):
                with (
                    self.subTest(name=name, mutated_sources=mutated_sources),
                    tempfile.TemporaryDirectory() as directory,
                ):
                    run_dir = Path(directory)
                    output = run_dir / "summary.json"
                    self.write_curve(run_dir)
                    paths = {
                        "console": run_dir
                        / "sealed-spacing32-converged-forward.console.log",
                        "runlog": run_dir
                        / "sealed-spacing32-converged-forward.run.log",
                    }
                    for source in mutated_sources:
                        path = paths[source]
                        path.write_text(mutate_validation(path.read_text(), replacement))

                    result = self.run_summarizer(run_dir, output)

                    self.assertEqual(result.returncode, 1)
                    self.assertFalse(output.exists())
                    self.assertIn("malformed full-atlas validation", result.stderr)

    def test_rejects_a_curve_without_the_authoritative_runtime_policy(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, include_policy=False)

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("authoritative runtime convergence policy", result.stderr)

    def test_rejects_each_runtime_policy_field_drift_from_the_contract(self) -> None:
        mutations = (
            {"absolute_threshold": 0.003},
            {"relative_threshold": 0.03},
            {"relative_floor": 0.06},
            {"consecutive_epochs": 3},
            {"minimum_update_epochs": 7},
            {"maximum_update_epochs": 64},
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                run_dir = Path(directory)
                output = run_dir / "summary.json"
                self.write_curve(run_dir, **mutation)

                result = self.run_summarizer(run_dir, output)

                self.assertEqual(result.returncode, 1)
                self.assertFalse(output.exists())
                self.assertIn("drifted from acceptance contract", result.stderr)

    def test_rejects_terminal_reason_that_disagrees_with_curve(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run_dir = Path(directory)
            output = run_dir / "summary.json"
            self.write_curve(run_dir, terminal_reason="SampleBudget")

            result = self.run_summarizer(run_dir, output)

        self.assertEqual(result.returncode, 1)
        self.assertFalse(output.exists())
        self.assertIn("terminal reason SampleBudget, expected Threshold", result.stderr)


if __name__ == "__main__":
    unittest.main()
