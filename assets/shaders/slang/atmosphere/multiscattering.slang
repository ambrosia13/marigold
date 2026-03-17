module multiscattering;

import "../lib/math";
import "../lib/atmosphere";

// Code taken and adapted from https://www.shadertoy.com/view/slSXRW

struct MultiscatteringResources {
    SamplerState sampler;
    Texture2D transmittanceTexture;
    RWTexture2D multiscatteringTexture;
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,

    uniform ParameterBlock<ConstantBuffer<AtmosphereParams>> atmosphereParams,
    uniform ParameterBlock<MultiscatteringResources> resources,
    
    uniform uint2 multiscatteringTextureSize,
) {
    float2 uv = float2(globalInvocationId.xy) / multiscatteringTextureSize;

    float u = uv.x;
    float v = uv.y;

    float celestialCosTheta = 2.0 * u - 1.0;
    float celestialTheta = safeAcos(celestialCosTheta);
    float height = lerp(atmosphereParams.groundRadius + 1e-6, atmosphereParams.atmosphereRadius, v);

    float3 pos = float3(0.0, height, 0.0);
    float3 celestialDir = normalize(float3(0.0, celestialCosTheta, -sin(celestialTheta)));

    float3 lum, f_ms;
    atmosphereParams.getMulScattValues(pos, celestialDir, resources.sampler, resources.transmittanceTexture, lum, f_ms);

    float3 psi = lum / (1.0 - f_ms);
    resources.multiscatteringTexture.Store(globalInvocationId.xy, float4(psi, 1.0));
}
