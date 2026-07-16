module composition_terrarium_panels;

import hash;
import projection;
import ray;
import tracer_types;

public static const float TERRARIUM_VOXELS_PER_CHUNK_AXIS = 256.0;
public static const float TERRARIUM_VOXEL_WORLD_SIZE = 1.0 / TERRARIUM_VOXELS_PER_CHUNK_AXIS;
public static const float TERRARIUM_GLASS_THICKNESS_VOXELS = 2.0;
public static const float TERRARIUM_GLASS_THICKNESS_WORLD =
    TERRARIUM_GLASS_THICKNESS_VOXELS * TERRARIUM_VOXEL_WORLD_SIZE;
public static const float TERRARIUM_GLASS_CORE_THICKNESS_WORLD =
    TERRARIUM_GLASS_THICKNESS_WORLD / 3.0;
public static const float TERRARIUM_PANEL_THICKNESS_VOXELS = 5.0;
public static const float TERRARIUM_PANEL_THICKNESS_WORLD =
    TERRARIUM_PANEL_THICKNESS_VOXELS * TERRARIUM_VOXEL_WORLD_SIZE;
public static const float TERRARIUM_GLASS_TOP_PADDING_WORLD = 0.08;
public static const int TERRARIUM_GLASS_MAX_HITS = 4;
public static const float TERRARIUM_GLASS_ALPHA = 0.092;
public static const float TERRARIUM_GLASS_VOXEL_GRID = TERRARIUM_VOXELS_PER_CHUNK_AXIS;
public static const uint TERRARIUM_GLASS_SSR_MAX_STEPS = 64u;
public static const float TERRARIUM_GLASS_SSR_MAX_DISTANCE_WORLD = 1.45;
public static const float TERRARIUM_GLASS_OCCLUSION_EPS_WORLD =
    TERRARIUM_VOXEL_WORLD_SIZE * 0.25;
public static const float2 TERRARIUM_BOX_MIN_XZ = float2(-TERRARIUM_GLASS_THICKNESS_WORLD);
public static const float2 TERRARIUM_BOX_MAX_XZ = float2(1.0 + TERRARIUM_GLASS_THICKNESS_WORLD);
public static const float TERRARIUM_FLOOR_Y = 0.0;
public static const float TERRARIUM_GLASS_TOP_Y = 1.0 + TERRARIUM_GLASS_TOP_PADDING_WORLD;
public static const float3 TERRARIUM_GLASS_BOX_MIN =
    float3(TERRARIUM_BOX_MIN_XZ.x, TERRARIUM_FLOOR_Y, TERRARIUM_BOX_MIN_XZ.y);
public static const float3 TERRARIUM_GLASS_BOX_MAX =
    float3(TERRARIUM_BOX_MAX_XZ.x, TERRARIUM_GLASS_TOP_Y, TERRARIUM_BOX_MAX_XZ.y);
public static const float3 TERRARIUM_BASE_BOX_MIN =
    float3(TERRARIUM_BOX_MIN_XZ.x,
           TERRARIUM_FLOOR_Y - TERRARIUM_PANEL_THICKNESS_WORLD,
           TERRARIUM_BOX_MIN_XZ.y);
public static const float3 TERRARIUM_BASE_BOX_MAX =
    float3(TERRARIUM_BOX_MAX_XZ.x, TERRARIUM_FLOOR_Y, TERRARIUM_BOX_MAX_XZ.y);
public static const float3 TERRARIUM_LID_BOX_MIN =
    float3(TERRARIUM_BOX_MIN_XZ.x, TERRARIUM_GLASS_TOP_Y, TERRARIUM_BOX_MIN_XZ.y);
public static const float3 TERRARIUM_LID_BOX_MAX =
    float3(TERRARIUM_BOX_MAX_XZ.x,
           TERRARIUM_GLASS_TOP_Y + TERRARIUM_PANEL_THICKNESS_WORLD,
           TERRARIUM_BOX_MAX_XZ.y);

public struct TerrariumPanelHit
{
    public bool hit;
    public float t;
    public float3 position;
    public float3 normal;
    public float3 box_min;
    public float3 box_max;
    public float layer;
};

public float sceneRayDistanceFromDepth(float2 screenUv, Ray ray,
                                       float depth, U_CameraInfo camera)
{
    float3 worldPosition = projectScreenUvToWorld(screenUv, depth,
                                                  camera.view_proj_mat_inv);
    return dot(worldPosition - ray.origin, ray.direction);
}

public float nearestSceneRayDistance(float2 screenUv, Ray ray,
                                     float rasterDepth, float computeDepth,
                                     float4 rasterColor, U_CameraInfo camera)
{
    float nearestDistance = 1.0e20;
    if (rasterColor.a > 0.0 && rasterDepth < 1.0)
    {
        nearestDistance = min(nearestDistance,
                              sceneRayDistanceFromDepth(screenUv, ray,
                                                        rasterDepth, camera));
    }
    if (computeDepth < 1.0)
    {
        nearestDistance = min(nearestDistance,
                              sceneRayDistanceFromDepth(screenUv, ray,
                                                        computeDepth, camera));
    }
    return nearestDistance;
}

TerrariumPanelHit makeEmptyTerrariumPanelHit()
{
    TerrariumPanelHit hit;
    hit.hit = false;
    hit.t = 1.0e20;
    hit.position = float3(0.0);
    hit.normal = float3(0.0, 1.0, 0.0);
    hit.box_min = float3(0.0);
    hit.box_max = float3(0.0);
    hit.layer = 0.0;
    return hit;
}

float3 terrariumBoxNormal(float3 position, float3 boxMinimum, float3 boxMaximum)
{
    const float epsilon = 1.0e-4;
    if (abs(position.x - boxMinimum.x) < epsilon) return float3(-1.0, 0.0, 0.0);
    if (abs(position.x - boxMaximum.x) < epsilon) return float3(1.0, 0.0, 0.0);
    if (abs(position.y - boxMinimum.y) < epsilon) return float3(0.0, -1.0, 0.0);
    if (abs(position.y - boxMaximum.y) < epsilon) return float3(0.0, 1.0, 0.0);
    if (abs(position.z - boxMinimum.z) < epsilon) return float3(0.0, 0.0, -1.0);
    return float3(0.0, 0.0, 1.0);
}

void tryTerrariumPanelBox(inout TerrariumPanelHit bestHit, Ray ray,
                          float3 boxMinimum, float3 boxMaximum,
                          float layer, float minimumDistance)
{
    float3 firstIntersection = (boxMinimum - ray.origin) * ray.inverse_direction;
    float3 secondIntersection = (boxMaximum - ray.origin) * ray.inverse_direction;
    float3 nearIntersections = min(firstIntersection, secondIntersection);
    float3 farIntersections = max(firstIntersection, secondIntersection);
    float nearDistance = max(max(nearIntersections.x, nearIntersections.y),
                             nearIntersections.z);
    float farDistance = min(min(farIntersections.x, farIntersections.y),
                            farIntersections.z);
    if (farDistance <= max(nearDistance, minimumDistance)) return;

    float distance = nearDistance > minimumDistance ? nearDistance : farDistance;
    if (distance <= minimumDistance || distance >= bestHit.t) return;

    float3 position = ray.origin + ray.direction * distance;
    bestHit.hit = true;
    bestHit.t = distance;
    bestHit.position = position;
    bestHit.normal = terrariumBoxNormal(position, boxMinimum, boxMaximum);
    bestHit.box_min = boxMinimum;
    bestHit.box_max = boxMaximum;
    bestHit.layer = layer;
}

public TerrariumPanelHit traceTerrariumPanels(Ray ray, float minimumDistance)
{
    TerrariumPanelHit hit = makeEmptyTerrariumPanelHit();
    tryTerrariumPanelBox(hit, ray, TERRARIUM_BASE_BOX_MIN,
                         TERRARIUM_BASE_BOX_MAX, 0.0, minimumDistance);
    tryTerrariumPanelBox(hit, ray, TERRARIUM_LID_BOX_MIN,
                         TERRARIUM_LID_BOX_MAX, 1.0, minimumDistance);
    return hit;
}

public float3 shadeTerrariumPanelHit(TerrariumPanelHit hit,
                              U_SunInfo sunInfo, U_CameraInfo camera)
{
    float3 normal = normalize(hit.normal);
    float3 viewDirection = normalize(camera.pos.xyz - hit.position);
    float normalView = clamp(dot(normal, viewDirection), 0.0, 1.0);
    float sun = max(dot(normal, normalize(sunInfo.sun_dir)), 0.0);
    float upFace = step(0.5, normal.y);
    float downFace = step(0.5, -normal.y);
    float sideFace = 1.0 - max(upFace, downFace);

    float3 localPosition = (hit.position - hit.box_min) /
                           max(hit.box_max - hit.box_min, float3(1.0e-5));
    float xEdge = min(localPosition.x, 1.0 - localPosition.x);
    float zEdge = min(localPosition.z, 1.0 - localPosition.z);
    float yEdge = min(localPosition.y, 1.0 - localPosition.y);
    float rimWidthX = TERRARIUM_PANEL_THICKNESS_WORLD /
                      max(hit.box_max.x - hit.box_min.x, 1.0e-5);
    float rimWidthZ = TERRARIUM_PANEL_THICKNESS_WORLD /
                      max(hit.box_max.z - hit.box_min.z, 1.0e-5);
    float rim = max(1.0 - step(rimWidthX, xEdge),
                    1.0 - step(rimWidthZ, zEdge));

    float3 walnut = float3(0.16, 0.105, 0.055);
    float3 darkTrim = float3(0.060, 0.050, 0.038);
    float3 warmEdge = float3(0.42, 0.285, 0.125);
    float3 topTint = lerp(float3(1.0), float3(0.86, 0.92, 1.0),
                          hit.layer * 0.18);
    float3 baseColor = lerp(walnut, darkTrim,
                            sideFace * 0.38 + downFace * 0.30) * topTint;
    baseColor = lerp(baseColor, warmEdge * topTint,
                     rim * (0.36 + upFace * 0.18));

    float3 grainCell = floor(hit.position * (TERRARIUM_VOXELS_PER_CHUNK_AXIS / 4.0));
    float grain = hash13(grainCell + float3(19.0, 47.0, hit.layer * 83.0));
    baseColor *= 0.96 + grain * 0.04;
    baseColor *= 0.55 + sun * 0.32 + normalView * 0.14 +
                 upFace * 0.08 - downFace * 0.10;
    baseColor += warmEdge * rim * sideFace *
                 (1.0 - step(0.18, yEdge)) * 0.10;
    return max(baseColor, float3(0.0));
}

public float3 applyTerrariumPanelsToScene(float3 sceneColor, Ray ray,
                                          float sceneDistance,
                                          U_SunInfo sun, U_CameraInfo camera)
{
    TerrariumPanelHit panelHit = traceTerrariumPanels(ray, 0.0);
    if (panelHit.hit &&
        panelHit.t + TERRARIUM_GLASS_OCCLUSION_EPS_WORLD < sceneDistance)
    {
        return shadeTerrariumPanelHit(panelHit, sun, camera);
    }
    return sceneColor;
}
