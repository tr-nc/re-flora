module scene_query;

import voxel_data;
import voxel_material;

public static const uint SCENE_QUERY_EVENT_NONE = 0u;
public static const uint SCENE_QUERY_EVENT_INTERFACE = 1u;
public static const uint SCENE_QUERY_EVENT_OPAQUE = 2u;
public static const uint SCENE_QUERY_EVENT_MISS = 3u;
public static const uint SCENE_QUERY_EVENT_STEP_BUDGET = 4u;

public static const uint SCENE_QUERY_MAX_ACTIVE_PATHS = 4u;
public static const uint SCENE_QUERY_MAX_INTERFACE_EVENTS = 8u;
public static const uint SCENE_QUERY_MAX_SCENE_QUERIES = 8u;
public static const uint SCENE_QUERY_MAX_DDA_STEPS = 2048u;
public static const float SCENE_QUERY_THROUGHPUT_CUTOFF = 1.0e-3;

static const float SCENE_QUERY_DIRECTION_EPSILON = 1.0e-20;
static const float SCENE_QUERY_TIE_EPSILON = 1.0e-5;
static const float SCENE_QUERY_VOXEL_BIAS = 1.0e-4;

public struct SceneQueryEvent
{
    public uint kind;
    public float3 position;
    public float3 incident_face_normal;
    public int3 from_cell;
    public int3 to_cell;
    public uint from_voxel_type;
    public uint to_voxel_type;
    public float medium_distance_world;
    public uint dda_steps;
    public uint tied_axes;
};

public struct ScenePrimaryQuery
{
    public uint has_glass_front;
    public SceneQueryEvent glass_front;
    public uint has_opaque_hit;
    public SceneQueryEvent opaque_hit;
    public uint scene_queries;
    public uint dda_steps;
    public uint exhausted;
};

public struct SceneQueryDiagnostics
{
    public uint scene_queries;
    public uint dda_steps;
    public uint interface_events;
    public uint peak_active_paths;
    public uint tir_events;
    public uint throughput_cutoffs;
    public uint top_k_pruned;
    public uint budget_exhaustions;
};

SceneQueryEvent makeSceneQueryEvent()
{
    SceneQueryEvent result;
    result.kind = SCENE_QUERY_EVENT_NONE;
    result.position = float3(0.0);
    result.incident_face_normal = float3(0.0);
    result.from_cell = int3(0);
    result.to_cell = int3(0);
    result.from_voxel_type = 0u;
    result.to_voxel_type = 0u;
    result.medium_distance_world = 0.0;
    result.dda_steps = 0u;
    result.tied_axes = 0u;
    return result;
}

ScenePrimaryQuery makeScenePrimaryQuery()
{
    ScenePrimaryQuery result;
    result.has_glass_front = 0u;
    result.glass_front = makeSceneQueryEvent();
    result.has_opaque_hit = 0u;
    result.opaque_hit = makeSceneQueryEvent();
    result.scene_queries = 0u;
    result.dda_steps = 0u;
    result.exhausted = 0u;
    return result;
}

bool sceneQueryCellInside(int3 cell, uint3 dimensions)
{
    return all(cell >= int3(0)) && all(cell < int3(dimensions));
}

uint sceneQueryVoxelType(readonly RWTexture3D<uint> denseAtlas,
                         int3 cell,
                         uint3 dimensions)
{
    if (!sceneQueryCellInside(cell, dimensions)) return 0u;
    return voxelTypeFromAtlasData(denseAtlas[uint3(cell)]);
}

bool sceneQuerySameMedium(VoxelMaterial left, uint leftVoxelType,
                          VoxelMaterial right, uint rightVoxelType)
{
    if (left.surface_class == VOXEL_SURFACE_CLASS_EMPTY &&
        right.surface_class == VOXEL_SURFACE_CLASS_EMPTY)
    {
        return true;
    }
    return left.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC &&
           right.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC &&
           leftVoxelType == rightVoxelType;
}

float sceneQueryAxisDelta(float direction)
{
    return abs(direction) <= SCENE_QUERY_DIRECTION_EPSILON
        ? 1.0e30
        : rcp(abs(direction));
}

float sceneQueryAxisNext(float origin, float direction, int cell, int stepDirection)
{
    if (stepDirection == 0) return 1.0e30;
    float boundary = float(stepDirection > 0 ? cell + 1 : cell);
    return (boundary - origin) / direction;
}

bool sceneQueryClipAxis(float origin, float direction, float dimension,
                        uint axis, inout float entry, inout float exit,
                        inout float3 entryNormal)
{
    if (abs(direction) <= SCENE_QUERY_DIRECTION_EPSILON)
    {
        return origin >= 0.0 && origin < dimension;
    }
    float first = -origin / direction;
    float second = (dimension - origin) / direction;
    float nearDistance = min(first, second);
    float farDistance = max(first, second);
    if (nearDistance > entry)
    {
        entry = nearDistance;
        entryNormal = float3(0.0);
        entryNormal[axis] = -sign(direction);
    }
    exit = min(exit, farDistance);
    return entry <= exit;
}

uint sceneQueryCrossingMask(float3 next, float crossing)
{
    float tolerance = SCENE_QUERY_TIE_EPSILON * max(abs(crossing), 1.0);
    uint mask = 0u;
    if (abs(next.x - crossing) <= tolerance) mask |= 1u;
    if (abs(next.y - crossing) <= tolerance) mask |= 2u;
    if (abs(next.z - crossing) <= tolerance) mask |= 4u;
    return mask;
}

uint sceneQueryNormalMask(float3 normal)
{
    uint mask = 0u;
    if (normal.x != 0.0) mask |= 1u;
    if (normal.y != 0.0) mask |= 2u;
    if (normal.z != 0.0) mask |= 4u;
    return mask;
}

float3 sceneQueryIncidentNormal(uint tiedAxes, int3 stepDirection)
{
    if ((tiedAxes & 1u) != 0u) return float3(-stepDirection.x, 0, 0);
    if ((tiedAxes & 2u) != 0u) return float3(0, -stepDirection.y, 0);
    if ((tiedAxes & 4u) != 0u) return float3(0, 0, -stepDirection.z);
    return float3(0.0);
}

void sceneQueryAdvanceTiedAxes(uint tiedAxes, int3 stepDirection,
                               float3 delta, inout int3 cell,
                               inout float3 next)
{
    if ((tiedAxes & 1u) != 0u) { cell.x += stepDirection.x; next.x += delta.x; }
    if ((tiedAxes & 2u) != 0u) { cell.y += stepDirection.y; next.y += delta.y; }
    if ((tiedAxes & 4u) != 0u) { cell.z += stepDirection.z; next.z += delta.z; }
}

// Finds the next true material transition or opaque terminal. Parameters are world-space;
// DDA parameters remain world distances even though traversal runs in atlas voxel coordinates.
public SceneQueryEvent walkVoxelMediaToNextEvent(
    readonly RWTexture3D<uint> denseAtlas,
    float3 originWorld,
    float3 directionWorld,
    float worldToVoxelScale,
    bool glassExperimentEnabled,
    uint stepBudget)
{
    SceneQueryEvent result = makeSceneQueryEvent();
    if (worldToVoxelScale <= 0.0 || stepBudget == 0u ||
        any(isnan(originWorld)) || any(isinf(originWorld)) ||
        any(isnan(directionWorld)) || any(isinf(directionWorld)) ||
        dot(directionWorld, directionWorld) <= SCENE_QUERY_DIRECTION_EPSILON)
    {
        result.kind = SCENE_QUERY_EVENT_STEP_BUDGET;
        return result;
    }

    uint width;
    uint height;
    uint depth;
    denseAtlas.GetDimensions(width, height, depth);
    uint3 dimensions = uint3(width, height, depth);
    float3 direction = normalize(directionWorld);
    float3 origin = originWorld * worldToVoxelScale;
    float3 voxelDirection = direction * worldToVoxelScale;

    float entry = -1.0e30;
    float exit = 1.0e30;
    float3 entryNormal = float3(0.0);
    if (!sceneQueryClipAxis(origin.x, voxelDirection.x, float(width), 0u,
                            entry, exit, entryNormal) ||
        !sceneQueryClipAxis(origin.y, voxelDirection.y, float(height), 1u,
                            entry, exit, entryNormal) ||
        !sceneQueryClipAxis(origin.z, voxelDirection.z, float(depth), 2u,
                            entry, exit, entryNormal) ||
        exit < max(entry, 0.0))
    {
        result.kind = SCENE_QUERY_EVENT_MISS;
        return result;
    }
    entry = max(entry, 0.0);

    float sampleEpsilon = SCENE_QUERY_VOXEL_BIAS / worldToVoxelScale;
    float sampleDistance = min(entry + sampleEpsilon, (entry + exit) * 0.5);
    float3 domainMax = float3(dimensions) - float3(SCENE_QUERY_VOXEL_BIAS);
    int3 cell = int3(floor(clamp(
        origin + voxelDirection * sampleDistance, float3(0.0), domainMax)));
    int3 stepDirection = int3(sign(voxelDirection));
    float3 delta = float3(
        sceneQueryAxisDelta(voxelDirection.x),
        sceneQueryAxisDelta(voxelDirection.y),
        sceneQueryAxisDelta(voxelDirection.z));
    float3 next = float3(
        sceneQueryAxisNext(origin.x, voxelDirection.x, cell.x, stepDirection.x),
        sceneQueryAxisNext(origin.y, voxelDirection.y, cell.y, stepDirection.y),
        sceneQueryAxisNext(origin.z, voxelDirection.z, cell.z, stepDirection.z));

    uint currentVoxelType = sceneQueryVoxelType(denseAtlas, cell, dimensions);
    VoxelMaterial currentMaterial = voxelMaterial(currentVoxelType, glassExperimentEnabled);
    if (entry > 0.0 && currentMaterial.surface_class != VOXEL_SURFACE_CLASS_EMPTY)
    {
        result.kind = currentMaterial.surface_class == VOXEL_SURFACE_CLASS_OPAQUE
            ? SCENE_QUERY_EVENT_OPAQUE
            : SCENE_QUERY_EVENT_INTERFACE;
        result.position = originWorld + direction * entry;
        result.incident_face_normal = entryNormal;
        result.from_cell = cell - stepDirection;
        result.to_cell = cell;
        result.from_voxel_type = 0u;
        result.to_voxel_type = currentVoxelType;
        result.tied_axes = sceneQueryNormalMask(entryNormal);
        return result;
    }

    float mediumDistance = 0.0;
    float currentDistance = entry;
    for (uint stepIndex = 0u; stepIndex < SCENE_QUERY_MAX_DDA_STEPS; ++stepIndex)
    {
        if (stepIndex >= stepBudget)
        {
            result.kind = SCENE_QUERY_EVENT_STEP_BUDGET;
            result.dda_steps = stepIndex;
            result.medium_distance_world = mediumDistance;
            return result;
        }
        result.dda_steps = stepIndex + 1u;
        float crossing = min(min(next.x, min(next.y, next.z)), exit);
        if (currentMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC)
        {
            mediumDistance += max(0.0, crossing - currentDistance);
        }
        uint tiedAxes = sceneQueryCrossingMask(next, crossing);
        float3 incidentNormal = sceneQueryIncidentNormal(tiedAxes, stepDirection);
        float3 position = originWorld + direction * crossing;
        if (crossing >= exit - SCENE_QUERY_TIE_EPSILON)
        {
            result.kind = currentMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC
                ? SCENE_QUERY_EVENT_INTERFACE
                : SCENE_QUERY_EVENT_MISS;
            result.position = position;
            result.incident_face_normal = incidentNormal;
            result.from_cell = cell;
            result.to_cell = cell + stepDirection;
            result.from_voxel_type = currentVoxelType;
            result.to_voxel_type = 0u;
            result.medium_distance_world = mediumDistance;
            result.tied_axes = tiedAxes;
            return result;
        }

        int3 previousCell = cell;
        sceneQueryAdvanceTiedAxes(
            tiedAxes, stepDirection, delta, cell, next);
        uint nextVoxelType = sceneQueryVoxelType(denseAtlas, cell, dimensions);
        VoxelMaterial nextMaterial = voxelMaterial(nextVoxelType, glassExperimentEnabled);
        if (nextMaterial.surface_class == VOXEL_SURFACE_CLASS_OPAQUE ||
            !sceneQuerySameMedium(
                currentMaterial, currentVoxelType, nextMaterial, nextVoxelType))
        {
            result.kind = nextMaterial.surface_class == VOXEL_SURFACE_CLASS_OPAQUE
                ? SCENE_QUERY_EVENT_OPAQUE
                : SCENE_QUERY_EVENT_INTERFACE;
            result.position = position;
            result.incident_face_normal = incidentNormal;
            result.from_cell = previousCell;
            result.to_cell = cell;
            result.from_voxel_type = currentVoxelType;
            result.to_voxel_type = nextVoxelType;
            result.medium_distance_world = mediumDistance;
            result.tied_axes = tiedAxes;
            return result;
        }
        currentVoxelType = nextVoxelType;
        currentMaterial = nextMaterial;
        currentDistance = crossing;
    }
    result.kind = SCENE_QUERY_EVENT_STEP_BUDGET;
    result.medium_distance_world = mediumDistance;
    return result;
}

public ScenePrimaryQuery tracePrimaryVoxelSurfaces(
    readonly RWTexture3D<uint> denseAtlas,
    float3 originWorld,
    float3 directionWorld,
    float worldToVoxelScale,
    bool glassExperimentEnabled)
{
    ScenePrimaryQuery result = makeScenePrimaryQuery();
    float3 direction = normalize(directionWorld);
    float3 origin = originWorld;
    float worldBias = SCENE_QUERY_VOXEL_BIAS / worldToVoxelScale;
    for (uint queryIndex = 0u; queryIndex < SCENE_QUERY_MAX_SCENE_QUERIES; ++queryIndex)
    {
        SceneQueryEvent event = walkVoxelMediaToNextEvent(
            denseAtlas, origin, direction, worldToVoxelScale,
            glassExperimentEnabled, SCENE_QUERY_MAX_DDA_STEPS);
        result.scene_queries += 1u;
        result.dda_steps += event.dda_steps;
        if (event.kind == SCENE_QUERY_EVENT_OPAQUE)
        {
            result.has_opaque_hit = 1u;
            result.opaque_hit = event;
            return result;
        }
        if (event.kind == SCENE_QUERY_EVENT_INTERFACE)
        {
            VoxelMaterial fromMaterial = voxelMaterial(
                event.from_voxel_type, glassExperimentEnabled);
            VoxelMaterial toMaterial = voxelMaterial(
                event.to_voxel_type, glassExperimentEnabled);
            if (result.has_glass_front == 0u &&
                (fromMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC ||
                 toMaterial.surface_class == VOXEL_SURFACE_CLASS_DIELECTRIC))
            {
                result.has_glass_front = 1u;
                result.glass_front = event;
            }
            origin = event.position + direction * worldBias;
            continue;
        }
        if (event.kind == SCENE_QUERY_EVENT_STEP_BUDGET)
        {
            result.exhausted = 1u;
        }
        return result;
    }
    result.exhausted = 1u;
    return result;
}

void sceneQueryIncidentFrame(float3 incidentDirection,
                             float3 incidentFaceNormal,
                             out float3 incident,
                             out float3 normal,
                             out float cosineIncident)
{
    incident = normalize(incidentDirection);
    normal = normalize(incidentFaceNormal);
    if (dot(incident, normal) > 0.0) normal = -normal;
    cosineIncident = clamp(-dot(incident, normal), 0.0, 1.0);
}

public bool sceneQueryRefractDielectric(float3 incidentDirection,
                                        float3 incidentFaceNormal,
                                        float etaIncident,
                                        float etaTransmitted,
                                        out float3 transmittedDirection)
{
    float3 incident;
    float3 normal;
    float cosineIncident;
    sceneQueryIncidentFrame(
        incidentDirection, incidentFaceNormal,
        incident, normal, cosineIncident);
    float eta = etaIncident / etaTransmitted;
    float discriminant = 1.0 - eta * eta *
        (1.0 - cosineIncident * cosineIncident);
    if (discriminant < 0.0)
    {
        transmittedDirection = float3(0.0);
        return false;
    }
    transmittedDirection = normalize(
        eta * incident +
        (eta * cosineIncident - sqrt(discriminant)) * normal);
    return true;
}

public float sceneQueryDielectricFresnel(float3 incidentDirection,
                                         float3 incidentFaceNormal,
                                         float etaIncident,
                                         float etaTransmitted)
{
    float3 incident;
    float3 normal;
    float cosineIncident;
    sceneQueryIncidentFrame(
        incidentDirection, incidentFaceNormal,
        incident, normal, cosineIncident);
    float eta = etaIncident / etaTransmitted;
    float sineTransmittedSquared = eta * eta *
        (1.0 - cosineIncident * cosineIncident);
    if (sineTransmittedSquared >= 1.0) return 1.0;
    float cosineTransmitted = sqrt(1.0 - sineTransmittedSquared);
    float parallel =
        (etaTransmitted * cosineIncident - etaIncident * cosineTransmitted) /
        (etaTransmitted * cosineIncident + etaIncident * cosineTransmitted);
    float perpendicular =
        (etaIncident * cosineIncident - etaTransmitted * cosineTransmitted) /
        (etaIncident * cosineIncident + etaTransmitted * cosineTransmitted);
    return saturate(0.5 *
        (parallel * parallel + perpendicular * perpendicular));
}

public float3 sceneQueryAbsorptionCoefficient(VoxelMaterial material)
{
    return -log(max(material.attenuation_color, float3(1.0e-6))) /
        max(material.attenuation_distance_world, 1.0e-6);
}

public float3 sceneQueryBeerLambert(float3 sigmaA, float distanceWorld)
{
    return exp(-max(sigmaA, float3(0.0)) * max(distanceWorld, 0.0));
}

public float sceneQueryThroughputLuminance(float3 throughput)
{
    return dot(throughput, float3(0.2126, 0.7152, 0.0722));
}
