import scene_pixel_filter;

bool close(float3 a, float3 b) { return all(abs(a - b) < 0.00001); }

export __extern_cpp int main()
{
    if (any(scenePixelExtent(uint2(320, 180), 1u) != uint2(320, 180))) return 1;
    if (any(scenePixelExtent(uint2(640, 360), 2u) != uint2(320, 180))) return 2;
    if (any(scenePixelExtent(uint2(1280, 720), 4u) != uint2(320, 180))) return 3;

    // Every screen pixel in an integer block maps to exactly the same coarse cell.
    for (uint stride = 1u; stride <= 8u; stride++) {
        uint2 grid = (uint2(2560, 1440) + stride - 1u) / stride;
        for (int y = 0; y < int(stride); y++)
            for (int x = 0; x < int(stride); x++)
                if (any(scenePixelCoordinate(int2(10,20)*int(stride)+int2(x,y), stride, grid) != int2(10,20))) return 4;
        if (any(scenePixelCoordinate(int2(2559,1439), stride, grid) != int2(grid)-1)) return 5;
    }

    // Odd windows retain partial final blocks rather than stretching or dropping them.
    uint2 grid = uint2(128,96);
    for (int y = 0; y < 767; y++)
        for (int x = 0; x < 1023; x++) {
            int2 pixel = scenePixelCoordinate(int2(x,y), 8u, grid);
            if (any(pixel != int2(x/8,y/8))) return 6;
            for (uint axis = 2u; axis <= 4u; axis++) {
                uint count = axis * axis;
                uint seen = 0u;
                for (uint sample = 0u; sample < count; sample++) {
                    int2 offset = scenePixelSampleOffset(sample, axis);
                    if (any(offset < 0) || any(offset >= int(axis))) return 7;
                    int2 tap = pixel * int(axis) + offset;
                    if (any(tap < 0) || any(tap >= int2(grid*axis))) return 8;
                    seen |= 1u << uint(offset.y * int(axis) + offset.x);
                }
                if (seen != ((1u << count) - 1u)) return 9;
            }
        }
    if (any(scenePixelCoordinate(int2(1022,766), 8u, grid) != int2(127,95))) return 10;
    if (any(scenePixelCoordinate(int2(0), 8u, uint2(1)) != int2(0))) return 11;

    // Coverage and unclamped HDR values are integrated at both requested qualities.
    if (!close(scenePixelBoxAverage(float3(4,8,12), 2u), float3(1,2,3))) return 12;
    if (!close(scenePixelBoxAverage(float3(1), 2u), float3(0.25))) return 13;
    if (!close(scenePixelBoxAverage(float3(1), 4u), float3(0.0625))) return 14;
    if (!close(scenePixelBoxAverage(float3(32,48,64), 4u), float3(2,3,4))) return 15;
    if (!close(scenePixelBoxAverage(float3(9,18,27), 3u), float3(1,2,3))) return 16;
    return 0;
}
