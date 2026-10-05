import model_view_quantization;
import model_pixel_types;

// Same immutable azimuths as the Rust upload. Only the CPU-test adapter generates
// them here; production reads a bound buffer, not vertex-stage sin/cos loops.
struct TestViewBank : IModelViewBank {
    float3 direction(uint index, uint count) {
        float angle = float(index) * (3.1415927410125732 * (3.0 - 2.2360680103302));
        return modelViewDirection(float2(cos(angle), sin(angle)), index, count);
    }
}
struct TieBank : IModelViewBank {
    float3 direction(uint index, uint count) { return float3(0, 0, 1); }
}
bool near(float3 a, float3 b) { return length(a - b) < 0.00002; }

export __extern_cpp int main() {
    TestViewBank bank;
    TieBank ties;
    if (modelViewCount(0u) != 8u || modelViewCount(0xffffffffu) != 512u) return 1;
    if (nearestModelView(float3(0, 0, 1), 128u, ties) != 0u) return 2;
    if (nearestModelView(float3(0), 256u, bank) != 0u) return 3;
    float invalid = asfloat(0x7fc00000u), infinity = asfloat(0x7f800000u);
    if (nearestModelView(float3(invalid, 0, 1), 128u, bank) != 0u
        || nearestModelView(float3(infinity, 1, 0), 128u, bank) != 0u) return 4;
    if (nearestModelView(float3(0, 1, 0), 128u, bank) != 0u
        || nearestModelView(float3(0, -1, 0), 256u, bank) != 255u) return 5;
    if (nearestModelView(float3(3e38), 128u, bank) != nearestModelView(float3(1), 128u, bank)) return 6;
    uint counts[7] = {8u, 9u, 37u, 128u, 256u, 511u, 512u};
    for (uint c = 0; c < 7; ++c) {
        uint count = counts[c];
        float meanY = 0;
        for (uint i = 0; i < count; ++i) {
            float3 p = bank.direction(i, count);
            if (abs(length(p) - 1.0) > 0.00001 || any(isnan(p)) || any(isinf(p))) return 7;
            if (nearestModelView(p, count, bank) != i) return 8;
            meanY += p.y;
        }
        if (abs(meanY / float(count)) > 0.000001) return 9;
        for (uint test = 0; test < 80; ++test) {
            float3 view = normalize(float3(sin(float(test)*0.73), cos(float(test)*0.27), cos(float(test)*0.91)));
            // Independent exhaustive reference, including the strict tie rule.
            float best = -2.0; uint expected = 0;
            for (uint i = 0; i < count; ++i) {
                float score = dot(view, bank.direction(i, count));
                if (score > best) { expected = i; best = score; }
            }
            if (nearestModelView(view, count, bank) != expected) return 10;
        }
    }
    if (near(bank.direction(64, 128), bank.direction(64, 256))) return 11;
    if (!near(modelViewRotateBetween(float3(0,1,0),float3(0,-1,0),float3(0,1,0)),float3(0,-1,0))) return 12;

    ModelPixelFrame physical = identityModelPixelFrame();
    physical.center = float3(7, 2, -3); physical.scale = 0.05;
    physical.axisX = float3(0,0,-1); physical.axisY = float3(0,1,0); physical.axisZ = float3(1,0,0);
    float3 localPivot = float3(0.4,1.2,-0.1);
    float3 pivot = modelViewWorldPoint(physical,localPivot);
    for (uint step = 0; step < 160; ++step) {
        // Includes both poles, rigid instance rotation and nonzero local pivot.
        float angle = float(step) * 6.2831853 / 159.0;
        float3 ray = normalize(float3(sin(angle), cos(angle), 0.2 * sin(angle)));
        float3 camera = pivot + ray;
        float3 localView = modelPixelLocalVector(physical,ray);
        uint index = nearestModelView(localView,128u,bank);
        ModelPixelFrame rendered = quantizeModelView(physical,localPivot,camera,true,128u,bank);
        if (!near(modelViewWorldPoint(rendered,localPivot),pivot)) return 13;
        if (!near(modelPixelLocalVector(rendered,ray),bank.direction(index,128u))) return 14;
        if (abs(dot(cross(rendered.axisX,rendered.axisY),rendered.axisZ)-1.0)>0.00001) return 15;
        if (abs(dot(rendered.axisX,rendered.axisY))>0.00001) return 16;
        float3 a = modelViewWorldPoint(rendered,localPivot+float3(1,0,0));
        float3 b = modelViewWorldPoint(rendered,localPivot+float3(0,1,0));
        float3 geometricNormal = normalize(cross(a-pivot,b-pivot));
        if (!near(geometricNormal,modelViewWorldVector(rendered,float3(0,0,1)))) return 17;
        ModelPixelFrame original = quantizeModelView(physical,localPivot,camera,false,128u,bank);
        if (any(original.center != physical.center) || any(original.axisX != physical.axisX)
            || any(original.axisY != physical.axisY) || any(original.axisZ != physical.axisZ)) return 18;
        if (any(camera != pivot+ray)) return 19;
        // Camera roll is intentionally not an input: no screen-axis/pole switch.
        if (any(isnan(rendered.axisX)) || any(isinf(rendered.axisX))) return 20;
    }
    for (uint i=0; i<3; ++i) {
        float3 camera = i==0?pivot:i==1?float3(invalid):float3(infinity);
        ModelPixelFrame unchanged = quantizeModelView(physical,localPivot,camera,true,0u,bank);
        if (any(unchanged.center != physical.center) || any(unchanged.axisX != physical.axisX)) return 21;
    }
    printf("model view quantization: actual-N bank, nearest/ties/invalid/poles, 160 rigid pivots, normals, handedness and unchanged camera passed\n");
    return 0;
}
