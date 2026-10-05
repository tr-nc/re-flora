module scene_depth;

public bool sceneHasDepth(float depth)
{
    return isfinite(depth) && depth >= 0.0 && depth < 1.0;
}
