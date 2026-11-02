import planar_normal_fit;
import occupancy_normal;

export __extern_cpp int main()
{
    int tested = 0;
    // Quantized plane samples, not exact analytical normals. Sweep inclination,
    // subvoxel phase, reflected directions and missing rows at material edges.
    for (int angle = -10; angle <= 10; ++angle)
        for (int phase = 0; phase < 16; ++phase)
            for (int edge = 0; edge < 2; ++edge)
            {
                float a = float(angle) / 10.0;
                float b = 0.0; // Wall bodies remain vertical on sloping ground.
                PlanarNormalFit fit = beginPlanarNormalFit();
                for (int j = -1; j <= 1; ++j)
                    for (int i = -4; i <= 4; ++i)
                    {
                        if (edge != 0 && j > 0) continue;
                        float u = float(i * 4), v = float(j * 8);
                        float h = floor(a*u + b*v + float(phase)/16.0);
                        addPlanarNormalPoint(fit, float3(u,v,h));
                    }
                float2 slope;
                if (!finishPlanarNormalFit(fit,slope)) { printf("rejected plane angle=%d phase=%d edge=%d\n",angle,phase,edge); return 1; }
                float3 actual = normalize(float3(-slope,1));
                float3 expected = normalize(float3(-a,-b,1));
                if (dot(actual,expected) < 0.9999) { printf("normal error angle=%d phase=%d edge=%d actual=(%f,%f)\n",angle,phase,edge,slope.x,slope.y); return 2; }
                tested++;
            }
    for (int shape = 0; shape < 4; ++shape)
    {
        PlanarNormalFit fit = beginPlanarNormalFit();
        for (int j = -2; j <= 2; ++j)
            for (int i = -2; i <= 2; ++i)
            {
                if (shape == 0 && j != 0) continue; // line
                if (shape == 1 && (i != 0 || j != 0)) continue; // point
                float u=float(i*6),v=float(j*6);
                float h=shape == 2 ? abs(u) : (i >= 0 ? 6.0 : 0.0);
                addPlanarNormalPoint(fit,float3(u,v,h));
            }
        float2 slope;
        if (finishPlanarNormalFit(fit,slope)) return 3+shape;
    }
    // Record whether radius-two reliability varies on the reported shallow wall.
    float minimumConfidence=1.0;
    float maximumOriginalAngle=0.0, maximumReconstructedAngle=0.0;
    float3 expectedWall=normalize(float3(0.19230769,0,1));
    for (int phase=0;phase<16;++phase)
    {
        OccupancyNormalMoments moments=beginOccupancyNormal();
        for(int z=-2;z<=2;++z) for(int y=-2;y<=2;++y)
        {
            uint row=0u;
            for(int x=-2;x<=2;++x)
                if(float(z) <= -0.19230769*float(x)+float(phase)/16.0)
                    row |= 1u << uint(x+2);
            accumulateOccupancyRow(row,y,z,moments);
        }
        float4 original=finishOccupancyNormal(moments);
        minimumConfidence=min(minimumConfidence,original.w);
        maximumOriginalAngle=max(maximumOriginalAngle,degrees(acos(clamp(dot(original.xyz,expectedWall),-1.0,1.0))));
        PlanarNormalFit fit=beginPlanarNormalFit();
        for(int j=-1;j<=1;++j) for(int i=-4;i<=4;++i)
        {
            float u=float(i*4),v=float(j*8);
            addPlanarNormalPoint(fit,float3(u,v,floor(-0.19230769*u+float(phase)/16.0)));
        }
        float2 slope;
        if(!finishPlanarNormalFit(fit,slope)) return 10;
        float3 reconstructed=normalize(float3(-slope.x,-slope.y,1));
        maximumReconstructedAngle=max(maximumReconstructedAngle,degrees(acos(clamp(dot(reconstructed,expectedWall),-1.0,1.0))));
    }
    printf("planar fit: %d quantized plane/phase/partial-support cases; folds, lines and points rejected; minimum original wall confidence=%f; maximum wall normal error original=%f degrees reconstructed=%f degrees\n",tested,minimumConfidence,maximumOriginalAngle,maximumReconstructedAngle);
    // Require sub-degree error for the reported wall, versus multi-degree
    // changes in its old radius-two field. This is not exact geometry recovery.
    return maximumOriginalAngle < 5.0 || maximumReconstructedAngle > 1.0 ? 11 : 0;
}
