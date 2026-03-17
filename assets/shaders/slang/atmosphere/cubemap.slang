module cubemap;

import "../lib/vertex";
import "../lib/atmosphere";

struct CubemapOutput {
    float4 face0: SV_Target0;
    float4 face1: SV_Target1;
    float4 face2: SV_Target2;
    float4 face3: SV_Target3;
    float4 face4: SV_Target4;
    float4 face5: SV_Target5;
}

[[shader("fragment")]]
CubemapOutput fragment( 
    VertexOutput input,

    uniform ParameterBlock<AtmosphereLuts> atmosphereLuts,
    uniform ParameterBlock<SkyViewLuts> skyViewLuts,
) {
    return CubemapOutput(
        float4(input.uv, 0.0, 1.0),
        float4(input.uv, 0.0, 1.0),
        float4(input.uv, 0.0, 1.0),
        float4(input.uv, 0.0, 1.0),
        float4(input.uv, 0.0, 1.0),
        float4(input.uv, 0.0, 1.0),
    );
}

