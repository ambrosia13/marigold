public static const float PI = 3.1415926535897932384626433832795;
public static const float HALF_PI = 1.57079632679489661923;
public static const float TAU = 6.2831853071795864769252867665590;

public static const float E = 2.718281828459045235360287471352;

public float pow2(float x) { return x * x; }
public float2 pow2(float2 x) { return x * x; }
public float3 pow2(float3 x) { return x * x; }
public float4 pow2(float4 x) { return x * x; }

public float safeAcos(float x) {
    return acos(clamp(x, -1.0, 1.0));
}

public float4 invertNormalizedQuaternion(float4 quat) {
    return float4(-quat.xyz, quat.w);
}

public float3 quaternionRotate(float4 quat, float3 v) {
    float3 axis = quat.xyz;
    float3 t = 2.0 * cross(axis, v);
    return v + quat.w * t + cross(axis, t);
}
