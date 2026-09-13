# Local leaf displacement range

The old Local Flutter Strength is wind torque gain, not a displacement limit:
its target angle saturates and the half-voxel hinge arm produces small excursions.
Increasing that gain alone cannot provide a useful 5-voxel position range.

Debug Panel → Flora → Leaves → Wind Motion now includes the saved declarative
`Local Flutter Displacement (voxels)` control, range 0–5. It scales only the
local leaf position, independently of optical turning, overall wind offset,
grass and sound. Inertial response and nonzero Flutter Strength are required.
The value is the maximum distance from the rest position, not peak-to-peak
distance or a guaranteed excursion in weak wind. Constant wind can settle to
an equilibrium; this does not introduce a periodic animation generator.

The original hinge excursion is normalized at the bounded 0.75-radian torque
target; transient overshoot is bounded before scaling. Default 0.36627253
preserves the original displacement below that reference angle. Maximum 5
offers approximately 13.65 times the original positional amplitude. Large
values are an artistic control and may visibly separate leaves from branches.

Slang regression coverage checks zero amplitude, zero angle, default
compatibility, a 5-voxel reference excursion and the displacement bound over
64 seeds and 201 angles. Existing generic save/reload and GUI grouping tests
cover the new declaration. GPU bindings and GUI fields are build-generated.

Validation: cargo fmt --check, cargo check, cargo test (979 main + 4 library,
2 ignored), and all 11 Slang CPU tests passed. The GPU-lock-protected release
hidden/muted smoke exited successfully; runtime log:
`target/re-flora-logs/re-flora-20260914-005205.253-195812.log`.
This is correctness/smoke evidence, not subjective animation acceptance.
Unrelated user audio configuration edits are preserved outside the commit.
