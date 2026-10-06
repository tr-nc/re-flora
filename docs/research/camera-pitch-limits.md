# Constrained vertical camera angles

## Finding

Games do restrict movable cameras' vertical angles. This is especially applicable to overhead/orbit cameras: horizontal movement and rotation can remain free while elevation is limited. The evidence does not establish 45 degrees as an industry-standard limit, or that every unrestricted first-person/free-flight camera uses such a narrow limit.

## Verified game example: 0 A.D.

The public [default configuration](https://github.com/0ad/0ad/blob/master/binaries/data/config/default.cfg) sets `view.rotate.x.min = 28`, `max = 60`, and `default = 35` degrees (the file expresses these as `rotate.x.*` within `[view]`). Its [camera controller](https://github.com/0ad/0ad/blob/master/source/graphics/CameraController.cpp) loads those limits and, when camera constraints are enabled, calls `m_RotateX.ClampSmoothly(DEGTORAD(min), DEGTORAD(max))` during updates. Thus this is a real restricted movable-camera implementation, not an unused configuration example. Both raw source files were retrieved.

The same file also contains `fov = 45`: that is the lens field of view, **not** the vertical camera tilt limit. The GitHub repository is an archived public mirror; these values describe that retrieved revision, not a verified current binary.

## Artistic rationale

[Battle Realms designer diary #5](https://www.gamespot.com/articles/battle-realms-designer-diary-5/1100-2677042/) describes a deliberately constrained overhead/slightly tilted view, with zoom-dependent tilt, to preserve readability and permit artists to optimize the environment for a limited set of viewpoints. The indexed developer account supports the artistic rationale; full-page retrieval returned HTTP 403. It does not establish a numerical 45-degree cap or a free-flight camera.

Engine-side pitch limits are also an explicit supported mechanism, e.g. Unreal's [PlayerCameraManager](https://dev.epicgames.com/documentation/en-us/unreal-engine/API/Runtime/Engine/APlayerCameraManager) and `LimitViewPitch`. Engine support is not evidence of a particular shipped game's camera settings.

## Re: Flora recommendation (not implemented)

Clarify the angle convention first: 0 degrees is horizontal, 45 degrees is an oblique downward view, and 90 degrees is straight down. Restricting upward head pitch is a different policy from restricting high-elevation/top-down grass viewing.

If the problem is grass appearing poor from increasingly overhead views, test an orbit/edit elevation ceiling of 45 degrees while retaining free horizontal rotation, panning, and zoom. Do not indiscriminately change the shared camera pitch clamp: Walking, free flight, and orbit have different purposes. Current orbit elevation permits approximately 87.7 degrees; the generic free-look clamp permits approximately +/-89.4 degrees.

A limit alone cannot protect appearance if camera position is independently unrestricted: a player can move above the garden and choose another focus point. Visual testing should therefore check the actual undesirable viewpoint and not confuse camera height with pitch. Existing snapshots and zoom/mode transitions must respect any future per-mode policy as well as mouse input.

For any implementation, avoid accumulating vertical mouse/smoothing displacement beyond the bound, so reversing input responds immediately. 45 degrees is a proposed artistic test value, not a research-derived requirement. No gameplay, settings, snapshots, or rendering code was changed for this investigation.
