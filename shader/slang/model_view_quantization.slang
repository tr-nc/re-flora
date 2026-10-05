// Pure rigid-view seam: no camera mutation, geometry, material or GPU resources.
module model_view_quantization;
import model_pixel_types;

public static const uint MODEL_VIEW_MIN = 8u;
public static const uint MODEL_VIEW_MAX = 512u;
public uint modelViewCount(uint requested) { return clamp(requested, MODEL_VIEW_MIN, MODEL_VIEW_MAX); }

// Azimuths may be shared; latitude MUST use the actual N, never a 512-bank prefix.
public float3 modelViewDirection(float2 azimuth, uint index, uint requested) {
    uint count = modelViewCount(requested);
    float y = 1.0 - 2.0 * (float(min(index, count - 1u)) + 0.5) / float(count);
    float radius = sqrt(max(0.0, 1.0 - y * y));
    return float3(azimuth.x * radius, y, azimuth.y * radius);
}
public interface IModelViewBank {
    float3 direction(uint index, uint count);
}

// Normalizing after max-component scaling also handles finite very large vectors.
public bool modelViewUnit(float3 input, out float3 unit) {
    unit = float3(0, 0, 1);
    if (any(isnan(input)) || any(isinf(input))) return false;
    float largest = max(abs(input.x), max(abs(input.y), abs(input.z)));
    if (largest <= 1e-20) return false;
    float3 scaled = input / largest;
    unit = scaled * rsqrt(dot(scaled, scaled));
    return true;
}
public uint nearestModelView<T : IModelViewBank>(float3 direction, uint requested, T bank) {
    float3 unit;
    if (!modelViewUnit(direction, unit)) return 0u;
    uint count = modelViewCount(requested), selected = 0u;
    float best = -2.0;
    for (uint index = 0u; index < count; ++index) {
        float score = dot(unit, bank.direction(index, count));
        // Deliberately strict: equal scores have a deterministic lowest-index owner.
        if (score > best) { best = score; selected = index; }
    }
    return selected;
}

// Shortest-arc transport preserves continuous roll and a right-handed rigid basis.
// Bank-nearest calls cannot be antipodal, but the generic seam defines it too.
public float3 modelViewRotateBetween(float3 from, float3 to, float3 vector) {
    float cosine = clamp(dot(from, to), -1.0, 1.0);
    if (cosine < -0.999999) {
        float3 a = abs(from);
        float3 reference = a.x <= a.y && a.x <= a.z ? float3(1, 0, 0)
            : a.y <= a.z ? float3(0, 1, 0) : float3(0, 0, 1);
        float3 axis = normalize(cross(from, reference));
        return 2.0 * axis * dot(axis, vector) - vector;
    }
    float3 c = cross(from, to);
    return vector + cross(c, vector) + cross(c, cross(c, vector)) / (1.0 + cosine);
}
public float3 modelViewWorldVector(ModelPixelFrame frame, float3 vector) {
    return frame.axisX * vector.x + frame.axisY * vector.y + frame.axisZ * vector.z;
}

public float3 modelViewWorldPoint(ModelPixelFrame frame, float3 point) {
    return frame.center + frame.scale * modelViewWorldVector(frame, point);
}

// physical must be a finite right-handed orthonormal frame with uniform scale.
// localPivot is in canonical mesh coordinates; all vertices of one rigid object
// must pass the SAME pivot/frame/camera/settings, never a per-vertex camera ray.
public ModelPixelFrame quantizeModelView<T : IModelViewBank>(ModelPixelFrame physical,
    float3 localPivot, float3 cameraPosition, bool enabled, uint requested, T bank) {
    if (!enabled) return physical; // Exact original path, including arithmetic.
    float3 pivot = modelViewWorldPoint(physical, localPivot);
    float3 worldView, localView;
    if (!modelViewUnit(cameraPosition - pivot, worldView)
        || !modelViewUnit(modelPixelLocalVector(physical, worldView), localView)) return physical;
    uint index = nearestModelView(localView, requested, bank);
    float3 chosen = bank.direction(index, modelViewCount(requested));
    ModelPixelFrame rendered = physical;
    rendered.axisX = modelViewWorldVector(physical, modelViewRotateBetween(chosen, localView, float3(1, 0, 0)));
    rendered.axisY = modelViewWorldVector(physical, modelViewRotateBetween(chosen, localView, float3(0, 1, 0)));
    rendered.axisZ = modelViewWorldVector(physical, modelViewRotateBetween(chosen, localView, float3(0, 0, 1)));
    rendered.center = pivot - physical.scale * modelViewWorldVector(rendered, localPivot);
    return rendered;
}
