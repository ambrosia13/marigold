module transmittance;

import "../lib/math";
import "../lib/atmosphere";

// Code taken and adapted from https://www.shadertoy.com/view/slSXRW

struct TransmittanceResources {
    RWTexture2D transmittanceTexture;
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,

    uniform ParameterBlock<ConstantBuffer<AtmosphereParams>> atmosphereParams,
    uniform ParameterBlock<TransmittanceResources> resources,

    uniform uint2 transmittanceTextureSize,
) {
    float2 uv = float2(globalInvocationId.xy) / transmittanceTextureSize;

    float u = uv.x;
    float v = uv.y;

    float celestialCosTheta = 2.0 * u - 1.0;
    float celestialTheta = safeAcos(celestialCosTheta);
    float height = lerp(atmosphereParams.groundRadius + 1e-6, atmosphereParams.atmosphereRadius, v);

    float3 pos = float3(0.0, height, 0.0);
    float3 celestialDir = normalize(float3(0.0, celestialCosTheta, -sin(celestialTheta)));

    float3 transmittance = atmosphereParams.getCelestialTransmittance(pos, celestialDir);

    resources.transmittanceTexture.Store(globalInvocationId.xy, float4(transmittance, 1.0));
}