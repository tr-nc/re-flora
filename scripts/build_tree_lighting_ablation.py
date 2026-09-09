#!/usr/bin/env python3
"""Build diagnostic-only Blacky ablations; always restore production sources/binary.

No experimental switch is shipped in the renderer. Timing binaries are separate
from the stats binary. Run from this worktree, with its default target directory.
"""
from pathlib import Path
import difflib
import hashlib
import json
import os
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'target/summer-evidence/blacky/ablation'
FILES = ('shader/slang/ddgi_query.slang', 'shader/slang/ddgi_voxel_visibility.slang',
         'shader/slang/tracer.slang')
QUERY, VISIBILITY, TRACER = FILES


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError(f'Expected one source seam: {old[:100]!r}')
    return source.replace(old, new)


def variant_sources(base, label):
    result = dict(base)
    query = base[QUERY]
    begin = query.index('DdgiQueryResult sampleDdgiTerrainSmoothEnvironmentFromAtlas(')
    end = query.index('public DdgiQueryResult sampleDdgiTerrainSmoothEnvironment(', begin)
    terrain = query[begin:end]
    if label == 'precull':
        # Keep the wrap weights of surviving probes unchanged. Restore only the
        # hard eligibility test, rather than confounding it with HARD weights.
        query = replace_once(query, '    float surfaceSideWeight;', '''    // [BLACKY_ABLATION] Hard eligibility only; preserve surviving weights.
    if ((spatialWeightMode & 8u) != 0u && surfaceAlignment <= 0.0)
    {
        contribution.rejection_flags |= DDGI_PROBE_REJECTION_SURFACE_SIDE_WEIGHT;
        return contribution;
    }
    float surfaceSideWeight;''')
        terrain_new = replace_once(terrain, 'DDGI_SPATIAL_WEIGHT_NOMINAL_WRAP, irradianceAtlas);',
                                   '(DDGI_SPATIAL_WEIGHT_NOMINAL_WRAP | 8u), irradianceAtlas);')
        query = query.replace(terrain, terrain_new)
    elif label == 'moments':
        terrain_new = replace_once(terrain, 'getDdgiUnoccludedSpatialWeightProbeContributionAt(',
                                   'getDdgiMomentSpatialWeightProbeContributionAt(')
        terrain_new = replace_once(terrain_new, '''float visibility = ddgiVoxelSegmentVisibility(
                    visibilityOrigin, contribution.actual_position,
                    query.geometry_revision);''', '''// [BLACKY_ABLATION] Same interpolation, statistical visibility only.
                float visibility = contribution.moment_visibility;''')
        query = query[:begin] + terrain_new + query[end:]
    elif label == 'stats':
        visibility = base[VISIBILITY]
        visibility = replace_once(visibility, '''public float ddgiVoxelSegmentVisibility(float3 biasedWorldPosition,
                                        float3 probeWorldPosition,
                                        uint expectedGeometryRevision)
{''', '''// [BLACKY_ABLATION] Diagnostic counters; excluded from timing binaries.
public float blackyVoxelSegmentVisibilityWithSteps(float3 biasedWorldPosition,
    float3 probeWorldPosition, uint expectedGeometryRevision,
    out uint steps)
{
    steps = 0u;''')
        visibility = replace_once(visibility, '''        if (stepIndex >= info.max_steps) return 0.0;''',
                                  '''        if (stepIndex >= info.max_steps) return 0.0;
        steps += 1u;''')
        visibility += '''
public float ddgiVoxelSegmentVisibility(float3 origin, float3 probe, uint revision)
{
    uint ignoredSteps;
    return blackyVoxelSegmentVisibilityWithSteps(origin, probe, revision, ignoredSteps);
}
'''
        result[VISIBILITY] = visibility
        query += '''
// [BLACKY_ABLATION] Re-evaluate exactly the production cage for a count-only
// capture. Current and precull rays share geometry, so precull is a subset.
public void blackyProbeTraversalStats(U_ShadingInfo lighting, float3 receiver,
    float3 positionWeightPosition, float3 normal, out float3 counts,
    out float4 extra)
{
    counts = float3(0.0);
    extra = float4(0.0);
    DdgiQueryInfo query = ddgiConsumerQueryInfo(lighting);
    if (ddgiQueryDomain(query, receiver) != DDGI_QUERY_DOMAIN_LOCAL) return;
    float3 origin = receiver + normal * ddgiVisibilityBiasWorld(query.visibility_bias_world);
    [unroll]
    for (uint z = 0u; z < 2u; ++z)
    {
        [unroll]
        for (uint y = 0u; y < 2u; ++y)
        {
            [unroll]
            for (uint x = 0u; x < 2u; ++x)
            {
                DdgiProbeContribution c = getDdgiUnoccludedSpatialWeightProbeContributionAt(
                    query, receiver, positionWeightPosition, normal, uint3(x,y,z),
                    DDGI_SPATIAL_WEIGHT_NOMINAL_WRAP, ddgi_irradiance_atlas);
                if (!c.trustworthy) continue;
                uint steps;
                float visible = blackyVoxelSegmentVisibilityWithSteps(
                    origin, c.actual_position, query.geometry_revision, steps);
                counts.x += 1.0;
                counts.y += float(steps);
                extra.y += visible;
                bool survives = dot(normal, c.actual_position - positionWeightPosition) > 0.0;
                if (survives)
                {
                    counts.z += 1.0;
                    extra.x += float(steps);
                    extra.z += visible;
                }
            }
        }
    }
}
'''
        result[TRACER] = replace_once(base[TRACER],
            '    environmentCaptureIrradiance = consumerResult.irradiance;', '''    environmentCaptureIrradiance = consumerResult.irradiance;
    // [BLACKY_ABLATION] Plane 0: calls/steps/precull-calls/hit.
    // Plane 1: precull-steps/visible-calls/precull-visible-calls/reserved.
    blackyProbeTraversalStats(shading_info, ddgiReceiverPosition, result.position,
        result.normal, environmentCaptureIrradiance, environmentCaptureWorld);''')
    else:
        raise ValueError(label)
    result[QUERY] = query
    return result


def build(label):
    env = dict(os.environ, CARGO_BUILD_JOBS='2')
    for args, name in [(['cargo', 'check'], 'check'),
                       (['cargo', 'build', '--release'], 'build')]:
        with (OUT / f'{label}-{name}.log').open('w') as log:
            subprocess.run(args, cwd=ROOT, env=env, stdout=log,
                           stderr=subprocess.STDOUT, check=True)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / 'bin').mkdir(exist_ok=True)
    base = {name: (ROOT / name).read_text() for name in FILES}
    for name, source in base.items():
        head = subprocess.check_output(['git', 'show', f'HEAD:{name}'], cwd=ROOT, text=True)
        if source != head:
            raise RuntimeError(f'Refusing to overwrite pre-existing edit: {name}')
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    manifest = dict(source_revision=revision, binaries={})
    try:
        build('current')
        shutil.copy2(ROOT / 'target/release/re-flora', OUT / 'bin/current')
        for label in ('precull', 'moments', 'stats'):
            print(f'[BLACKY_ABLATION] building {label}', flush=True)
            sources = variant_sources(base, label)
            patch = ''
            for name, source in sources.items():
                patch += ''.join(difflib.unified_diff(base[name].splitlines(True),
                    source.splitlines(True), fromfile=f'a/{name}', tofile=f'b/{name}'))
                if (ROOT / name).read_text() != source:
                    (ROOT / name).write_text(source)
            (OUT / f'{label}.patch').write_text(patch)
            build(label)
            shutil.copy2(ROOT / 'target/release/re-flora', OUT / 'bin' / label)
    finally:
        for name, source in base.items():
            if (ROOT / name).read_text() != source:
                (ROOT / name).write_text(source)
        print('[BLACKY_ABLATION] restoring production binary', flush=True)
        build('restored')
    for label in ('current', 'precull', 'moments', 'stats'):
        path = OUT / 'bin' / label
        manifest['binaries'][label] = dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
    manifest['restored_sha256'] = hashlib.sha256((ROOT / 'target/release/re-flora').read_bytes()).hexdigest()
    if manifest['restored_sha256'] != manifest['binaries']['current']['sha256']:
        raise RuntimeError('Restored binary differs from the original production build')
    (OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print('[BLACKY_ABLATION] sources and default release restored', flush=True)


if __name__ == '__main__':
    main()
