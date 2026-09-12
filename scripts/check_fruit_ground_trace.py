#!/usr/bin/env python3
"""Check the final ten simulated seconds of an opt-in real-game fruit trace."""
import argparse
import collections
import json
import math


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace")
    args = parser.parse_args()
    bodies = collections.defaultdict(list)
    with open(args.trace, encoding="utf-8") as source:
        for line in source:
            row = json.loads(line)
            bodies[row["body"]].append(row)
    end = max((rows[-1]["simulated"] for rows in bodies.values()), default=0)
    passed = end >= 30
    results = []
    for body, rows in bodies.items():
        if rows[-1]["simulated"] < end - 0.1:
            continue  # A crop retired during the initial re-arm is not the final drop.
        tail = [row for row in rows if row["simulated"] >= end - 10]
        position_range = [max(r["position"][i] for r in tail) - min(r["position"][i] for r in tail) for i in range(3)]
        rotation = tail[0]["rotation"]
        angle_range = max(2 * math.acos(min(1, abs(sum(a*b for a,b in zip(rotation, r["rotation"]))) / (math.sqrt(sum(v*v for v in rotation)) * math.sqrt(sum(v*v for v in r["rotation"]))))) for r in tail)
        render_error = max(max(abs(p/256 - v) for p,v in zip(r["position"], r["render_position"])) for r in rows)
        render_rotation_error = max(max(abs(p-v) for p,v in zip(r["rotation"], r["render_rotation"])) for r in rows)
        sleeping = sum(r["sleeping"] for r in tail)
        first_sleep = next((r["simulated"] for r in rows if r["sleeping"]), None)
        ok = (tail[0]["simulated"] <= end - 9.9 and max(position_range) < 0.01
              and angle_range < 0.01 and sleeping == len(tail)
              and render_error < 1e-7 and render_rotation_error < 1e-7)
        passed &= ok
        results.append(dict(body=body, samples=len(tail), position_range_voxels=position_range,
                            angle_range_radians=angle_range, sleeping_samples=sleeping,
                            first_sleep_seconds=first_sleep, missing_solver_contacts=sum(r["solver_contacts"] == 0 for r in tail),
                            render_position_error=render_error, render_rotation_error=render_rotation_error,
                            max_linear_speed=max(math.sqrt(sum(v*v for v in r["linvel"])) for r in tail),
                            max_angular_speed=max(math.sqrt(sum(v*v for v in r["angvel"])) for r in tail),
                            pending_terrain_bricks=max(r["dirty_bricks"] for r in tail), passed=ok))
    passed &= bool(results)
    print(json.dumps(dict(passed=passed, simulated_seconds=end, bodies=results), indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
