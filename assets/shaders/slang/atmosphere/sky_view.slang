module sky_view;

import "../lib/screen";
import "../lib/math";
import "../lib/atmosphere";

// Code taken and adapted from https://www.shadertoy.com/view/slSXRW

struct SkyViewResources {
    RWTexture2D dayTexture;
    RWTexture2D nightTexture;
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,

    uniform ParameterBlock<Screen> screen,
    uniform ParameterBlock<ConstantBuffer<AtmosphereParams>> atmosphereParams,
    uniform ParameterBlock<AtmosphereLuts> atmosphereLuts,
    uniform ParameterBlock<SkyViewResources> resources,
    
    uniform uint2 skyViewTextureSize,
) {
    // don't render sky view LUT if not inside atmosphere
    if (!atmosphereParams.isInsideAtmosphere(screen.camera.position) || atmosphereParams.isInsideGround(screen.camera.position)) {
        resources.dayTexture.Store(globalInvocationId.xy, float4(1.0, 0.0, 0.0, 1.0));
        resources.nightTexture.Store(globalInvocationId.xy, float4(0.0, 1.0, 0.0, 1.0));
        return;
    }

    float2 uv = float2(globalInvocationId.xy) / skyViewTextureSize;

    float u = uv.x;
    float v = uv.y;

    float clipV = v * 2.0 - 1.0;

    float azimuthAngle = (u - 0.5) * 2.0 * PI;

    float adjV;
    if (v < 0.5) {
        float coord = -clipV;
        adjV = -coord * coord;
    } else {
        float coord = clipV;
        adjV = coord * coord;
    }

    float3 skyViewPos = atmosphereParams.worldPosToLocalPos(screen.camera.position);

    float height = length(skyViewPos);
    float3 up = skyViewPos / height;
    
    float horizonAngle = safeAcos(sqrt(height * height - atmosphereParams.groundRadius * atmosphereParams.groundRadius) / height) - HALF_PI;
    float altitudeAngle = adjV * HALF_PI - horizonAngle;
    float cosAltitude = cos(altitudeAngle);

    float3 rayDir = float3(cosAltitude * sin(azimuthAngle), sin(altitudeAngle), -cosAltitude * cos(azimuthAngle));

    float sunAltitude = HALF_PI - safeAcos(dot(atmosphereParams.sunDirection, up));
    float3 sunDir = float3(0.0, sin(sunAltitude), -cos(sunAltitude));

    float moonAltitude = HALF_PI * safeAcos(dot(atmosphereParams.moonDirection, up));
    float3 moonDir = float3(0.0, sin(moonAltitude), -cos(moonAltitude));

    float atmoDist = rayIntersectSphere(skyViewPos, rayDir, atmosphereParams.atmosphereRadius);
    float groundDist = rayIntersectSphere(skyViewPos, rayDir, atmosphereParams.groundRadius);

    float tMax = groundDist < 0.0 ? atmoDist : groundDist;

    float3 dayColor = atmosphereParams.raymarchScattering(
        skyViewPos,
        rayDir,
        sunDir,
        atmosphereParams.sunColor,
        tMax,
        atmosphereLuts
    );

    float3 nightColor = atmosphereParams.raymarchScattering(
        skyViewPos,
        rayDir,
        moonDir,
        atmosphereParams.moonColor * atmosphereParams.moonToSunIlluminanceRatio,
        tMax,
        atmosphereLuts
    );

    resources.dayTexture.Store(globalInvocationId.xy, float4(dayColor, 1.0));
    resources.nightTexture.Store(globalInvocationId.xy, float4(nightColor, 1.0));
}

