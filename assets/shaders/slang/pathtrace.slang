module pathtrace;

import "lib/pass/geometry";
import "lib/screen";
import "lib/random";
import "lib/math";
import "lib/intersect";
import "lib/scene";

struct Background {
    SamplerState sampler;
    TextureCube texture;
}

float3 accumulate(float3 current, float3 previous, inout float frameAge, in Screen screen) {
    if (screen.view.frameCount > 0 && screen.shouldAccumulate()) {
        float3 accumulated = (previous * frameAge + current) / (frameAge + 1.0);
        frameAge += 1.0;

        return accumulated;
    } else {
        // reset frame age
        frameAge = 0.0;
        return current;
    }
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,
    uniform ParameterBlock<Background> background,
    uniform ParameterBlock<Scene> scene,
    uniform ParameterBlock<GeometryTextures> textures,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
    if (globalInvocationId.x >= screen.view.width || globalInvocationId.y >= screen.view.height) {
        return;
    }

    uint3 groupId = globalInvocationId - localInvocationId;
    uint pixelId = groupId.x + groupId.y * screen.view.width / 8;
    uint frameOffset = screen.view.width * screen.view.height * screen.view.frameCount;
    Random random = Random(pixelId + frameOffset);

    float2 texcoord = float2(globalInvocationId.xy) / float2(screen.view.width, screen.view.height);
    float3 viewDir = screen.camera.viewDir(texcoord);

    Ray ray = Ray(screen.camera.position, viewDir);

    float3 color = background.texture.SampleLevel(background.sampler, viewDir, 0).rgb;
    
    uint3 traversalInfo = uint3(0);

    // Hit hit = Hit.NO_HIT;
    // Mesh mesh = scene.meshes[0];

    // for (int i = 0; i < mesh.triangleCount; i++) {
    //     hit = Hit.merge(hit, scene.rayIntersectTriangle(ray, mesh.vertexOffset, mesh.triangleOffset + i));
    // }



    // Hit hit = scene.raytraceTlas(ray, traversalInfo);
    // color = float3(traversalInfo) / 500.0;

    // if (hit.success) {
    //     color = max(float3(0.0), hit.normal);
    // } else {
    //     color = float3(traversalInfo) / 10.0;
    // }


    float3 radiance = float3(0.0);
    float3 throughput = float3(1.0);

    uint bounces = 5;
    for (uint i = 0; i < bounces; i++) {
        Hit hit = scene.raytraceTlas(ray, traversalInfo);

        if (!hit.success) {
            radiance += throughput * background.texture.SampleLevel(background.sampler, ray.dir, 0).rgb;
            break;
        } else {
            ray = Ray(
                hit.biasPositionPositive(),
                random.getCosineVector(hit.normal)
            );

            // throughput *= float3(0.95);
        }
    }

    color = radiance;

    float4 previousSample = textures.previous.Load(int3(globalInvocationId.xy, 0));
    float3 previousColor = previousSample.rgb;
    float frameAge = previousSample.a;

    color = accumulate(color, previousColor, frameAge, screen);

    // if (hit.success) {
    //     color = float3(1.0, 0.0, 0.0) * (hit.normal.y * 0.5 + 0.5);
    // } else {
    // }

    // Mesh mesh = scene.meshes[0];
    // BvhNode rootNode = scene.blasNodes[mesh.blasRoot];
    // BvhNode rootChildA = scene.blasNodes[mesh.blasRoot + rootNode.childNode];
    // BvhNode rootChildB = scene.blasNodes[mesh.blasRoot + rootNode.childNode + 1];
    
    // color = float3(bool3(
    //     rayIntersectBoundingVolume(ray, rootNode.boundsMin, rootNode.boundsMax).success,
    //     rayIntersectBoundingVolume(ray, rootChildA.boundsMin, rootChildA.boundsMax).success,
    //     rayIntersectBoundingVolume(ray, rootChildB.boundsMin, rootChildB.boundsMax).success
    // ));

    // color = float3(0.0);

    // for (uint i = 0; i < scene.counts.blasNodeCount; i++) {
    //     BvhNode node = scene.blasNodes[i];
    //     color += float(rayIntersectBoundingVolume(ray, node.boundsMin, node.boundsMax).success) / 10.0;
    // }
    
    // color = float3(0.0);
    // while (true) {
    //     color += float(rayIntersectBoundingVolume(ray, rootNode.boundsMin, rootNode.boundsMax).success) / 10.0;
    //     if (rootNode.childNode == 0) break;
    //     rootNode = scene.blasNodes[mesh.blasRoot + rootNode.childNode];
    // }

    // color = float3(float(rayIntersectBoundingVolume(ray, float3(-0.5), float3(0.5)).success));

    textures.current.Store(globalInvocationId.xy, float4(color, frameAge));
}