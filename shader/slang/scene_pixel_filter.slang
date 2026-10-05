// Resolve sampling density independently of the coarse displayed pixel grid.
public uint2 scenePixelExtent(uint2 renderExtent, uint samplesPerAxis)
{
    return renderExtent / samplesPerAxis;
}

public int2 scenePixelCoordinate(int2 outputPixel, uint pixelStride, uint2 pixelExtent)
{
    // Exact screen-pixel groups. The last block may be cropped by the window;
    // never stretch a rounded-down grid across the output extent.
    return min(outputPixel / int(pixelStride), int2(pixelExtent) - 1);
}

public int2 scenePixelSampleOffset(uint sample, uint samplesPerAxis)
{
    return int2(sample % samplesPerAxis, sample / samplesPerAxis);
}

// Average linear HDR samples before tone mapping; never mix adjacent coarse cells.
public float3 scenePixelBoxAverage(float3 sum, uint samplesPerAxis)
{
    return sum / float(samplesPerAxis * samplesPerAxis);
}
