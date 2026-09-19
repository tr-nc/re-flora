# Debug panel scrollbar and response groups (2026-09-16)

Root cause: the panel configured ScrollSource::MOUSE_WHEEL, which leaves the
bar visible but sets scroll_bar=false. The bar therefore only sensed hover;
its drag fell through to the movable egui Window.

The diagnosing-bugs regression uses the production scroll-area builder inside
an actual movable egui Window and sends pointer press/move/release events.
Before fixing, a 60-point drag produced offset=0 and window_delta=[0,60].
After enabling MOUSE_WHEEL | SCROLL_BAR, content scrolls and the window stays
put. Both solid and the game's floating-bar styling are covered. Content-drag
scrolling remains disabled so it does not compete with curve editing.

Wind retains Generation / Response. Response now groups Grass, Leaves, Sound,
and genuinely shared mechanical controls. The amplitude/frequency graphs remain
directly inside the relevant object group, without their own collapsing menus.
All four curve captions now use ASCII wording such as “Flutter Amplitude vs Wind”.
Existing parameter values and persistence remain untouched.

Validation:

- cargo test scrollbar_drag: original red recorded in /tmp/debug-scroll-red.log;
  two drag variants green in /tmp/debug-scroll-green.log.
- cargo fmt --check, cargo check, cargo test: 994 main + 4 library passed,
  2 ignored; /tmp/debug-panel-final-tests.log.
- Locked release hidden muted 0.5-second smoke passed, failures=0:
  target/re-flora-logs/re-flora-20260916-012754.105-69632.log.
- UI pointer behavior is tested in egui; hidden startup is native/GPU smoke,
  not a claim of manual visible-window acceptance.
- No shader/generated/config changes, no dependency publication, merge or push.
