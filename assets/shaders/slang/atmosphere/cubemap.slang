module cubemap;

import "../lib/vertex";
import "../lib/screen";
import "../lib/atmosphere";

struct CubemapOutput {
    float4 face0: SV_Target0;
    float4 face1: SV_Target1;
    float4 face2: SV_Target2;
    float4 face3: SV_Target3;
    float4 face4: SV_Target4;
    float4 face5: SV_Target5;
}

float3[6] getCubemapViewDirs(float2 uv) {
    float3 a = normalize(float3(uv - 0.5, 0.5));

    float3 viewDirs[6];

	viewDirs[0] = float3( a.z, -a.y, -a.x);
	viewDirs[1] = float3(-a.z, -a.y,  a.x);
	viewDirs[2] = float3( a.x,  a.z,  a.y);
	viewDirs[3] = float3( a.x, -a.z, -a.y);
	viewDirs[4] = float3( a.x, -a.y,  a.z);
	viewDirs[5] = float3(-a.x, -a.y, -a.z);

    return viewDirs;
}

float4 render(float3 viewDir, Screen screen, AtmosphereParams atmosphereParams, AtmosphereLuts atmosphereLuts, SkyViewLuts skyViewLuts) {
    float3 color = float3(0.0);
    
    float3 skyViewPos = atmosphereParams.worldPosToLocalPos(screen.camera.position);
    float height = length(skyViewPos);
    bool isInsideAtmosphere = height < atmosphereParams.atmosphereRadius;
    bool isInsideGround = height < atmosphereParams.groundRadius;

    if (isInsideGround) {
        return float4(color, 1.0);
    } else if (isInsideAtmosphere) {
        color += atmosphereParams.getValFromSkyLUT(
            atmosphereParams.worldPosToLocalPos(screen.camera.position),
            viewDir * float3(1.0, -1.0, 1.0),
            atmosphereParams.sunDirection,
            skyViewLuts.sampler,
            skyViewLuts.day
        );

        color += atmosphereParams.getValFromSkyLUT(
            atmosphereParams.worldPosToLocalPos(screen.camera.position),
            viewDir * float3(1.0, -1.0, 1.0),
            atmosphereParams.moonDirection,
            skyViewLuts.sampler,
            skyViewLuts.night
        );
    } else {
        viewDir.y *= -1.0;

        float atmoDistFar; 
        float atmoDistNear = rayIntersectSphere(skyViewPos, viewDir, atmosphereParams.atmosphereRadius, atmoDistFar);
        float groundDist = rayIntersectSphere(skyViewPos, viewDir, atmosphereParams.groundRadius);

        float tMin, tMax;

        if (atmoDistFar < 0.0) {
            // not looking at atmosphere at all, skip trace
            return float4(color, 1.0);
        } else if (atmoDistNear < 0.0) {
            // inside atmosphere
            tMin = 0.0; // start from camera
            tMax = groundDist < 0.0 ? atmoDistFar : groundDist; // trace until either the ground, or the edge of atmosphere
        } else {
            // outside atmosphere
            tMin = atmoDistNear; // start from near edge of atmosphere
            tMax = groundDist < 0.0 ? atmoDistFar : groundDist; // trace until either the ground, or the far edge of atmosphere
        }

        float3 transmittance;

        color += atmosphereParams.raymarchScattering(
            skyViewPos,
            viewDir,
            atmosphereParams.sunDirection,
            atmosphereParams.sunColor,
            tMin,
            tMax,
            atmosphereLuts,
            transmittance
        );
        
        color += atmosphereParams.raymarchScattering(
            skyViewPos,
            viewDir,
            atmosphereParams.moonDirection,
            atmosphereParams.moonColor * atmosphereParams.moonToSunIlluminanceRatio,
            tMin,
            tMax,
            atmosphereLuts,
            transmittance
        );

        // color += atmosphereParams.groundAlbedo * transmittance * float(groundDist >= 0.0) / atmosphereParams.brightnessMultiplier;
    }
    
    color *= atmosphereParams.brightnessMultiplier;

    return float4(color, 1.0);
}

[[shader("fragment")]]
CubemapOutput fragment( 
    VertexOutput input,

    uniform ParameterBlock<Screen> screen,
    uniform ParameterBlock<AtmosphereParams> atmosphereParams,
    uniform ParameterBlock<AtmosphereLuts> atmosphereLuts,
    uniform ParameterBlock<SkyViewLuts> skyViewLuts,
) {
    float3 viewDirs[6] = getCubemapViewDirs(input.uv);

    return CubemapOutput(
        render(viewDirs[0], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
        render(viewDirs[1], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
        render(viewDirs[2], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
        render(viewDirs[3], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
        render(viewDirs[4], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
        render(viewDirs[5], screen, atmosphereParams, atmosphereLuts, skyViewLuts),
    );
}

