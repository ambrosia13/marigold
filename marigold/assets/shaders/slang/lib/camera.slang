module camera;

public struct CameraUniform {
    public float4x4 viewProjectionMatrix;
    public float4x4 viewMatrix;
    public float4x4 projectionMatrix;

    public float4x4 inverseViewProjectionMatrix;
    public float4x4 inverseViewMatrix;
    public float4x4 inverseProjectionMatrix;

    public float4x4 previousViewProjectionMatrix;
    public float4x4 previousViewMatrix;
    public float4x4 previousProjectionMatrix;

    public float3 position;
    public float3 previousPosition;

    public float3 view;
    public float3 previousView;

    public float3 right;
    public float3 up;
}
