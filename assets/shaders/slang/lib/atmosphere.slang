module atmosphere;

import "math";

// Code taken and adapted from https://www.shadertoy.com/view/slSXRW

public struct AtmosphereParams {
    public float3 sunColor;
    public float groundRadius;
    public float3 moonColor;
    public float atmosphereRadius;

    public float3 rayleighScatteringBase;
    public float rayleighAbsorptionBase;

    public float3 ozoneAbsorptionBase;
    public float mieScatteringBase;

    public float3 groundAlbedo;
    public float mieAbsorptionBase;

    public float3 origin;
    public float atmosphereG;

    public float3 sunDirection;
    public float moonToSunIlluminanceRatio;

    public float3 moonDirection;
    public float metersPerUnit;

    public float rayleighScaleHeight;
    public float mieScaleHeight;

    public uint transmittanceLutSteps;
    public uint multiscatteringLutSteps;
    public uint multiscatteringLutSqrtSamples;
    public uint skyViewLutSteps;

    public float3 worldPosToLocalPos(float3 pos) {
        return (pos - origin) / metersPerUnit;
    }

    public bool isInsideAtmosphere(float3 worldSpacePos) {
        return length(worldPosToLocalPos(worldSpacePos)) <= atmosphereRadius;
    }

    public bool isInsideGround(float3 worldSpacePos) {
        return length(worldPosToLocalPos(worldSpacePos)) <= groundRadius;
    }

    public float getMiePhase(float cosTheta) {
        return kleinNishinaPhase(cosTheta, this.atmosphereG);
    }

    public void getScatteringValues(
        float3 pos,
        out float3 rayleighScattering,
        out float mieScattering,
        out float3 extinction
    ) {
        float altitude = length(pos) - groundRadius;

        float rayleighDensity = exp(-altitude / rayleighScaleHeight);
        float mieDensity = exp(-altitude / mieScaleHeight);

        rayleighScattering = rayleighScatteringBase * rayleighDensity;
        float rayleighAbsorption = rayleighAbsorptionBase * rayleighDensity;

        mieScattering = mieScatteringBase * mieDensity;
        float mieAbsorption = mieAbsorptionBase * mieDensity;

        float3 ozoneAbsorption = ozoneAbsorptionBase * max(0.0, 1.0 - abs(altitude * 1000.0 - 25.0) / 15.0);
        
        extinction = rayleighScattering + rayleighAbsorption + mieScattering + mieAbsorption + ozoneAbsorption;
    }

    public float3 getCelestialTransmittance(float3 pos, float3 celestialDir) {
        if (rayIntersectSphere(pos, celestialDir, groundRadius) > 0.0) {
            return float3(0.0);
        }

        float atmoDist = rayIntersectSphere(pos, celestialDir, atmosphereRadius);
        float t = 0.0;

        float3 transmittance = float3(1.0);
        for (float i = 0.0; i < float(transmittanceLutSteps); i += 1.0) {
            float newT = ((i + 0.3) / float(transmittanceLutSteps)) * atmoDist;
            float dt = newT - t;
            t = newT;

            float3 newPos = pos + t * celestialDir;

            float3 rayleighScattering, extinction;
            float mieScattering;
            getScatteringValues(newPos, rayleighScattering, mieScattering, extinction);

            transmittance *= exp(-dt * extinction);
        }

        return transmittance;
    }

    public float3 getValFromTLUT(SamplerState sampler, Texture2D texture, float3 pos, float3 celestialDir) {
        float height = length(pos);
        float3 up = pos / height;
        float sunCosZenithAngle = dot(celestialDir, up);

        float2 uv = float2(
            clamp(0.5 + 0.5 * sunCosZenithAngle, 0.0, 1.0),
            clamp((height - groundRadius) / (atmosphereRadius - groundRadius), 0.0, 1.0)
        );

        return texture.SampleLevel(sampler, uv, 0).rgb;
    }

    float3 getSphericalDir(float theta, float phi) {
        float cosPhi = cos(phi);
        float sinPhi = sin(phi);
        float cosTheta = cos(theta);
        float sinTheta = sin(theta);
        return float3(sinPhi*sinTheta, cosPhi, sinPhi*cosTheta);
    }

    // Calculates Equation (5) and (7) from the paper.
    public void getMulScattValues(float3 pos, float3 celestialDir, SamplerState sampler, Texture2D transmittanceTexture, out float3 lumTotal, out float3 fms) {
        lumTotal = float3(0.0);
        fms = float3(0.0);
        
        float invSamples = 1.0 / float(multiscatteringLutSqrtSamples * multiscatteringLutSqrtSamples);
        for (int i = 0; i < multiscatteringLutSqrtSamples; i++) {
            for (int j = 0; j < multiscatteringLutSqrtSamples; j++) {
                // This integral is symmetric about theta = 0 (or theta = PI), so we
                // only need to integrate from zero to PI, not zero to 2*PI.
                float theta = PI * (float(i) + 0.5) / float(multiscatteringLutSqrtSamples);
                float phi = safeAcos(1.0 - 2.0 * (float(j) + 0.5) / float(multiscatteringLutSqrtSamples));
                float3 rayDir = getSphericalDir(theta, phi);
                
                float atmoDist = rayIntersectSphere(pos, rayDir, atmosphereRadius);
                float groundDist = rayIntersectSphere(pos, rayDir, groundRadius);
                float tMax = atmoDist;
                if (groundDist > 0.0) {
                    tMax = groundDist;
                }
                
                float cosTheta = dot(rayDir, celestialDir);
        
                float miePhaseValue = getMiePhase(cosTheta);
                float rayleighPhaseValue = getRayleighPhase(-cosTheta);
                
                float3 lum = float3(0.0), lumFactor = float3(0.0), transmittance = float3(1.0);
                float t = 0.0;
                for (float stepI = 0.0; stepI < float(multiscatteringLutSteps); stepI += 1.0) {
                    float newT = ((stepI + 0.3) / float(multiscatteringLutSteps)) * tMax;
                    float dt = newT - t;
                    t = newT;

                    float3 newPos = pos + t*rayDir;

                    float3 rayleighScattering, extinction;
                    float mieScattering;
                    getScatteringValues(newPos, rayleighScattering, mieScattering, extinction);

                    float3 sampleTransmittance = exp(-dt * extinction);
                    
                    // Integrate within each segment.
                    float3 scatteringNoPhase = rayleighScattering + mieScattering;
                    float3 scatteringF = (scatteringNoPhase - scatteringNoPhase * sampleTransmittance) / extinction;
                    lumFactor += transmittance * scatteringF;
                    
                    // This is slightly different from the paper, but I think the paper has a mistake?
                    // In equation (6), I think S(x,w_s) should be S(x-tv,w_s).
                    float3 sunTransmittance = getValFromTLUT(sampler, transmittanceTexture, newPos, celestialDir);

                    float3 rayleighInScattering = rayleighScattering * rayleighPhaseValue;
                    float mieInScattering = mieScattering * miePhaseValue;
                    float3 inScattering = (rayleighInScattering + mieInScattering) * sunTransmittance;

                    // Integrated scattering within path segment.
                    float3 scatteringIntegral = (inScattering - inScattering * sampleTransmittance) / extinction;

                    lum += scatteringIntegral * transmittance;
                    transmittance *= sampleTransmittance;
                }
                
                if (groundDist > 0.0) {
                    float3 hitPos = pos + groundDist*rayDir;
                    if (dot(pos, celestialDir) > 0.0) {
                        hitPos = normalize(hitPos) * groundRadius;
                        lum += transmittance * groundAlbedo * getValFromTLUT(sampler, transmittanceTexture, hitPos, celestialDir);
                    }
                }
                
                fms += lumFactor * invSamples;
                lumTotal += lum * invSamples;
            }
        }
    }

    public float3 getValFromMultiScattLUT(SamplerState sampler, Texture2D texture, float3 pos, float3 celestialDir) {
        float height = length(pos);
        float3 up = pos / height;
        float sunCosZenithAngle = dot(celestialDir, up);

        float2 uv = float2(
            clamp(0.5 + 0.5 * sunCosZenithAngle, 0.0, 1.0),
            clamp((height - groundRadius) / (atmosphereRadius - groundRadius), 0.0, 1.0)
        );

        return texture.SampleLevel(sampler, uv, 0).rgb;
    }

    public float3 raymarchScattering(
        float3 pos,
        float3 rayDir,
        float3 celestialDir,
        float3 celestialColor,
        float tMax,
        AtmosphereLuts luts
    ) {
        float cosTheta = dot(rayDir, sunDirection);

        float miePhaseValue = getMiePhase(cosTheta);
        float rayleighPhaseValue = getRayleighPhase(cosTheta);

        float3 lum = float3(0.0);
        float3 transmittance = float3(1.0);

        float t = 0.0;
        for (float i = 0.0; i < float(skyViewLutSteps); i += 1.0) {
            float newT = ((i + 0.3) / float(skyViewLutSteps)) * tMax;
            float dt = newT - t;
            t = newT;

            float3 newPos = pos + t * rayDir;

            float3 rayleighScattering, extinction;
            float mieScattering;
            getScatteringValues(newPos, rayleighScattering, mieScattering, extinction);

            float3 sampleTransmittance = exp(-dt * extinction);

            float3 celestialTransmittance = getValFromTLUT(luts.sampler, luts.transmittance, newPos, celestialDir) * celestialColor;
            float3 psiMS = getValFromMultiScattLUT(luts.sampler, luts.multiscattering, newPos, celestialDir) * celestialColor;

            float3 rayleighInScattering = rayleighScattering * (rayleighPhaseValue * celestialTransmittance + psiMS);
            float3 mieInScattering = mieScattering * (miePhaseValue * celestialTransmittance + psiMS);
            float3 inScattering = rayleighInScattering + mieInScattering;

            float3 scatteringIntegral = (inScattering - inScattering * sampleTransmittance) / extinction;

            lum += scatteringIntegral * transmittance;
            transmittance *= sampleTransmittance;
        }

        return lum;
    }

    public float3 getValFromSkyLUT(float3 pos, float3 rayDir, float3 celestialDir, SamplerState sampler, Texture2D texture) {
        float height = length(pos);
        float3 up = pos / height;

        float horizonAngle = safeAcos(sqrt(height * height - groundRadius * groundRadius) / height);
        float altitudeAngle = horizonAngle - acos(dot(rayDir, up));
        
        float azimuthAngle;
        if (abs(altitudeAngle) > HALF_PI - 0.0001) {
            azimuthAngle = 0.0;
        } else {
            float3 right = cross(celestialDir, up);
            float3 forward = cross(up, right);

            float3 projectedDir = normalize(rayDir - up * dot(rayDir, up));
            float sinTheta = dot(projectedDir, right);
            float cosTheta = dot(projectedDir, forward);
            azimuthAngle = atan2(sinTheta, cosTheta) + PI;
        }

        float v = 0.5 + 0.5 * sign(altitudeAngle) * sqrt(abs(altitudeAngle) * 2.0 / PI);
        float2 uv = float2(azimuthAngle / TAU, v);

        return texture.SampleLevel(sampler, uv, 0).rgb;
    }
}

public struct AtmosphereLuts {
    public SamplerState sampler;
    public Texture2D transmittance;
    public Texture2D multiscattering;
}

public struct SkyViewLuts {
    public SamplerState sampler;
    public Texture2D day;
    public Texture2D night;
}

public float kleinNishinaPhase(float cosTheta, float g) {
	float e = 1.0;
	for (int i = 0; i < 8; ++i) {
		float gFromE = 1.0 / e - 2.0 / log(2.0 * e + 1.0) + 1.0;
		float deriv = 4.0 / ((2.0 * e + 1.0) * pow2(log(2.0 * e + 1.0))) - 1.0 / pow2(e);
		if (abs(deriv) < 0.00000001) break;
		e = e - (gFromE - g) / deriv;
	}

	return e / (2.0 * PI * (e * (1.0 - cosTheta) + 1.0) * log(2.0 * e + 1.0));
}

public float getRayleighPhase(float cosTheta) {
    return 0.05968310365 * (1.0 + cosTheta * cosTheta);
}

public float rayIntersectSphere(float3 ro, float3 rd, float rad) {
    float b = dot(ro, rd);
    float c = dot(ro, ro) - rad * rad;
    if (c > 0.0 && b > 0.0) return -1.0;
    float discr = b * b - c;
    if (discr < 0.0) return -1.0;
    // Special case: inside sphere, use far discriminant
    if (discr > b * b) return (-b + sqrt(discr));
    return -b - sqrt(discr);    
}
