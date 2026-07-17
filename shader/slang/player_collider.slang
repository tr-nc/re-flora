import contree_node;
import marching_result;
import ray;
import scene_marching;

static const uint WORKGROUP_SIZE = 64u;
static const uint NUM_RING_DISTANCES = 32u;
static const int RAY_HALF_KERNEL_SIZE = 1;
static const int KERNEL_DIM = 2 * RAY_HALF_KERNEL_SIZE + 1;
static const uint NUM_GROUND_RAYS = uint(KERNEL_DIM * KERNEL_DIM);
static const uint NUM_CEILING_RAYS = uint(KERNEL_DIM * KERNEL_DIM);
static const uint TOTAL_RAYS = NUM_GROUND_RAYS + NUM_CEILING_RAYS + NUM_RING_DISTANCES;
static const float RAY_OFFSET = 1.0 / 256.0;
static const float COLLISION_RAY_DISTANCE = 2.0;
static const float TWO_PI = 6.28318530716;

struct U_PlayerColliderInfo
{
    float3 player_pos;
    float3 camera_front;
};

[[vk::binding(0, 0)]]
ConstantBuffer<U_PlayerColliderInfo> player_collider_info;

[[vk::binding(1, 0)]]
StructuredBuffer<ContreeNode> contree_node_data;

[[vk::binding(2, 0)]]
StructuredBuffer<uint> contree_leaf_data;

[[vk::binding(3, 0)]]
[[vk::image_format("rg32ui")]]
readonly RWTexture3D<uint2> scene_tex;

struct B_PlayerCollisionResult
{
    float ground_distance;
    float ceiling_distance;
    float ring_distances[NUM_RING_DISTANCES];
};

[[vk::binding(4, 0)]]
writeonly GLSLShaderStorageBuffer<B_PlayerCollisionResult, Std430DataLayout>
    player_collision_result;

groupshared float ground_results[KERNEL_DIM][KERNEL_DIM];
groupshared float ceiling_results[KERNEL_DIM][KERNEL_DIM];
groupshared float ring_collision_distances[NUM_RING_DISTANCES];

MarchingResult generalSceneMarching(Ray ray, uint localInvocationIndex)
{
    return marchScene(contree_node_data, contree_leaf_data, scene_tex,
                      ray.origin, ray.direction, ray.inverse_direction,
                      localInvocationIndex);
}

float3 getRingDirection(int ringIndex)
{
    float3 flattenedFront = normalize(float3(player_collider_info.camera_front.x, 0.0,
                                              player_collider_info.camera_front.z));
    float3 right = normalize(cross(flattenedFront, float3(0.0, 1.0, 0.0)));

    if (ringIndex == 0)
    {
        return flattenedFront;
    }

    float angle = TWO_PI * float(ringIndex - 1) / float(NUM_RING_DISTANCES - 1u);
    return normalize(flattenedFront * cos(angle) + right * sin(angle));
}

float gaussianWeight(int x, int y)
{
    const float sigma = float(RAY_HALF_KERNEL_SIZE) * 0.5 + 0.5;
    float sigmaSquared = sigma * sigma;
    return exp(-float(x * x + y * y) / (2.0 * sigmaSquared));
}

[shader("compute")]
[numthreads(WORKGROUP_SIZE, 1, 1)]
void main(uint3 dispatchThreadId : SV_DispatchThreadID,
          uint localInvocationIndex : SV_GroupIndex)
{
    uint id = dispatchThreadId.x;

    if (id < NUM_GROUND_RAYS)
    {
        int x = int(id % uint(KERNEL_DIM)) - RAY_HALF_KERNEL_SIZE;
        int y = int(id / uint(KERNEL_DIM)) - RAY_HALF_KERNEL_SIZE;

        float2 offset = float2(float(x), float(y)) * RAY_OFFSET;
        Ray ray;
        ray.origin = player_collider_info.player_pos + float3(offset.x, 0.0, offset.y);
        ray.direction = float3(0.0, -1.0, 0.0);
        ray.inverse_direction = 1.0 / ray.direction;

        MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
        ground_results[x + RAY_HALF_KERNEL_SIZE][y + RAY_HALF_KERNEL_SIZE] =
            result.distance;
    }
    else if (id < NUM_GROUND_RAYS + NUM_CEILING_RAYS)
    {
        int ceilingId = int(id - NUM_GROUND_RAYS);
        int x = ceilingId % KERNEL_DIM - RAY_HALF_KERNEL_SIZE;
        int y = ceilingId / KERNEL_DIM - RAY_HALF_KERNEL_SIZE;

        float2 offset = float2(float(x), float(y)) * RAY_OFFSET;
        Ray ray;
        ray.origin = player_collider_info.player_pos + float3(offset.x, 0.0, offset.y);
        ray.direction = float3(0.0, 1.0, 0.0);
        ray.inverse_direction = 1.0 / ray.direction;

        MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
        float distance = result.is_hit
            ? min(result.distance, COLLISION_RAY_DISTANCE)
            : COLLISION_RAY_DISTANCE;
        ceiling_results[x + RAY_HALF_KERNEL_SIZE][y + RAY_HALF_KERNEL_SIZE] = distance;
    }
    else if (id < TOTAL_RAYS)
    {
        int ringIndex = int(id - NUM_GROUND_RAYS - NUM_CEILING_RAYS);
        float3 direction = getRingDirection(ringIndex);

        Ray ray;
        ray.origin = player_collider_info.player_pos;
        ray.direction = direction;
        ray.inverse_direction = 1.0 / ray.direction;

        MarchingResult result = generalSceneMarching(ray, localInvocationIndex);
        float distance = result.is_hit
            ? min(result.distance, COLLISION_RAY_DISTANCE)
            : COLLISION_RAY_DISTANCE;
        ring_collision_distances[ringIndex] = distance;
    }

    // Keep every invocation in the 64-thread workgroup active through the
    // barrier. Only the first 50 cast rays, but Vulkan barriers must execute in
    // uniform control flow across the whole workgroup.
    GroupMemoryBarrierWithGroupSync();

    if (id == 0u)
    {
        float weightedSum = 0.0;
        float sumOfWeights = 0.0;
        for (int i = -RAY_HALF_KERNEL_SIZE; i <= RAY_HALF_KERNEL_SIZE; ++i)
        {
            for (int j = -RAY_HALF_KERNEL_SIZE; j <= RAY_HALF_KERNEL_SIZE; ++j)
            {
                float weight = gaussianWeight(i, j);
                float groundDistance = ground_results[i + RAY_HALF_KERNEL_SIZE]
                                                     [j + RAY_HALF_KERNEL_SIZE];
                weightedSum += groundDistance * weight;
                sumOfWeights += weight;
            }
        }
        player_collision_result.ground_distance =
            weightedSum / max(sumOfWeights, 1.0e-8);

        float minimumCeilingDistance = COLLISION_RAY_DISTANCE;
        for (int i = 0; i < KERNEL_DIM; ++i)
        {
            for (int j = 0; j < KERNEL_DIM; ++j)
            {
                minimumCeilingDistance =
                    min(minimumCeilingDistance, ceiling_results[i][j]);
            }
        }
        player_collision_result.ceiling_distance = minimumCeilingDistance;

        for (uint i = 0u; i < NUM_RING_DISTANCES; ++i)
        {
            player_collision_result.ring_distances[i] = ring_collision_distances[i];
        }
    }
}
