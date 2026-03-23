module screen;

public struct Screen {
    public ConstantBuffer<Camera> camera;
    public ConstantBuffer<View> view;

    public bool shouldAccumulate() {
        return 
            all(this.camera.position == this.camera.previousPosition) &&
            all(this.camera.view == this.camera.previousView) && 
            all(this.camera.projectionMatrix[0] == this.camera.previousProjectionMatrix[0]);
    }
}

public struct Camera {
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

    static float3 fromScreenSpace(float3 screenSpacePos, float4x4 matrix) {
        float3 clipSpacePos = float3(
            screenSpacePos.x * 2.0 - 1.0,
            1.0 - screenSpacePos.y * 2.0,
            screenSpacePos.z * 2.0 - 1.0
        );
        float4 temp = mul(matrix, float4(clipSpacePos, 1.0));
        return temp.xyz / temp.w;
    }

    static float3 toScreenSpace(float3 pos, float4x4 matrix) {
        float4 temp = mul(matrix, float4(pos, 1.0));
        return (temp.xyz / temp.w) * 0.5 + 0.5;
    }

    public float3 screenToScene(float3 screenSpacePos) {
        float3 worldSpacePos = fromScreenSpace(screenSpacePos, this.inverseViewProjectionMatrix);
        return worldSpacePos - this.position;
    }

    public float3 viewDir(float2 texcoord) {
        float3 screenSpacePos = float3(texcoord, 1.0);
        float3 sceneSpacePos = this.screenToScene(screenSpacePos);
        return normalize(sceneSpacePos);
    }
}

public struct View {
    public uint width;
    public uint height;
    public float aspectRatio;

    public uint frameCount;
}
