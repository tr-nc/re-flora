import ddgi_query;

export __extern_cpp int main()
{
    // A coplanar probe must illuminate air on the layer as well as immediately
    // above/below it. Averaging +Y/-Y surface queries instead loses all support
    // exactly at alignment zero; making the fog denser only amplifies that seam.
    for (uint mode = DDGI_SPATIAL_WEIGHT_RELOCATION_NO_SURFACE;
         mode <= DDGI_SPATIAL_WEIGHT_NOMINAL_NO_SURFACE; ++mode)
    {
        for (int i = -100; i <= 100; ++i)
        {
            float alignment = float(i) / 100.0;
            if (ddgiSurfaceSideWeight(alignment,mode) != 1.0) return 1;
            if (ddgiSurfaceSideWeight(alignment,mode) !=
                ddgiSurfaceSideWeight(-alignment,mode)) return 2;
        }
    }
    // Existing opaque surface callers retain their same-side/wrap contracts.
    if (ddgiSurfaceSideWeight(0,0u) != 0.0 ||
        ddgiSurfaceSideWeight(-1,0u) != 0.0 ||
        ddgiSurfaceSideWeight(1,0u) != 1.0) return 3;
    if (abs(ddgiSurfaceSideWeight(0,2u)-0.45) > 0.00001 ||
        abs(ddgiSurfaceSideWeight(-1,2u)-0.2) > 0.00001 ||
        abs(ddgiSurfaceSideWeight(1,2u)-1.2) > 0.00001) return 4;
    return 0;
}
