import lib.screen;
import lib.pass.post;

struct Menu {
    ConstantBuffer<MenuData> data;
    StructuredBuffer<TextLine> left;
    StructuredBuffer<TextLine> right;
}

struct MenuData {
    uint charPadding;
    uint linePadding;
    uint leftLineCount;
    uint rightLineCount;
}

struct TextLine {
    uint lineIndex;
    uint charCount;
    uint2 chars[128];
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,

    uniform ParameterBlock<PostTextures> textures,
    uniform ParameterBlock<Menu> menu,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
    if (globalInvocationId.x >= screen.view.width || globalInvocationId.y >= screen.view.height) {
        return;
    }

    float2 texcoord = float2(globalInvocationId.xy) / float2(screen.view.width, screen.view.height);

    textures.output.Store(globalInvocationId.xy, float4(texcoord, 0.0, 1.0));
}