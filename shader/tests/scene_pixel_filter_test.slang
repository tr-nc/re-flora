import scene_pixel_filter;

bool close(float3 a, float3 b) { return all(abs(a - b) < 0.00001); }

export __extern_cpp int main()
{
    if (any(scenePixelExtent(uint2(320, 180), 1u) != uint2(320, 180))) return 1;
    if (any(scenePixelExtent(uint2(640, 360), 2u) != uint2(320, 180))) return 2;
    if (any(scenePixelExtent(uint2(254, 190), 2u) != uint2(127, 95))) return 3;

    uint2 screen = uint2(2560, 1440), grid = uint2(320, 180);
    // All 64 output pixels in a nearest-upscaled block share one coarse cell.
    for (int y = 0; y < 8; y++)
        for (int x = 0; x < 8; x++)
            if (any(scenePixelCoordinate(int2(80+x, 160+y), screen, grid) != int2(10, 20))) return 4;
    if (any(scenePixelCoordinate(int2(2559, 1439), screen, grid) != int2(319, 179))) return 5;

    // Odd window dimensions preserve the grid and keep every source tap in bounds.
    screen = uint2(1023, 767); grid = uint2(127, 95);
    for (int y = 0; y < 767; y++)
        for (int x = 0; x < 1023; x++) {
            int2 pixel = scenePixelCoordinate(int2(x, y), screen, grid);
            if (any(pixel < 0) || any(pixel >= int2(grid))) return 6;
            if (any(pixel * 2 + 1 >= int2(254, 190))) return 7;
            // Off-mode mapping is the original point-upscale formula.
            float2 ratio = float2(grid) / float2(screen);
            int2 original = min(int2((float2(x, y) + 0.5) * ratio), int2(grid) - 1);
            if (any(original != pixel)) return 8;
        }
    if (any(scenePixelCoordinate(int2(0), uint2(1), uint2(1)) != int2(0))) return 9;

    // Every tap contributes, including subpixel coverage and unclamped HDR values.
    if (!close(scenePixelBoxAverage(float3(4,0,0), float3(0,8,0), float3(0,0,12), float3(0)), float3(1,2,3))) return 10;
    if (!close(scenePixelBoxAverage(float3(1), float3(0), float3(0), float3(0)), float3(0.25))) return 11;
    if (!close(scenePixelBoxAverage(float3(2,3,4), float3(2,3,4), float3(2,3,4), float3(2,3,4)), float3(2,3,4))) return 12;
    return 0;
}
