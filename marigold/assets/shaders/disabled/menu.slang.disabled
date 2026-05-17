import lib.screen;
import lib.pass.post;

static const uint BASE_CHAR_SIZE = 8;

static const float3 FOREGROUND_COLOR = float3(1.0);
static const float3 BACKGROUND_COLOR = float3(0.0);
static const float FOREGROUND_ALPHA = 1.0;
static const float BACKGROUND_ALPHA = 0.5;

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
    float scale;
}

struct TextLine {
    uint lineIndex;
    uint charCount;
    uint2 chars[128];
}

void renderMenu(
    inout float4 color, 
    uint2 globalIndex, 
    uint screenWidth,
    MenuData menuData, 
    StructuredBuffer<TextLine> lines, 
    uint lineCount, 
    bool flipX = false
) {
    if (flipX) globalIndex.x = screenWidth - 1 - globalIndex.x;

    for (uint i = 0; i < lineCount; i++) {
        TextLine line = lines[i];

        uint integerScale = uint(menuData.scale);
        uint totalLinePadding = menuData.linePadding * line.lineIndex * integerScale;
        uint2 adjustedGlobalIndex = globalIndex - uint2(0, totalLinePadding);

        float2 menuCoord = float2(adjustedGlobalIndex.xy);
        menuCoord /= menuData.scale;

        // correct aspect ratio
        menuCoord.x *= 1.0 / 0.75;

        float2 cellCoord = (menuCoord % float2(BASE_CHAR_SIZE)) / float2(BASE_CHAR_SIZE);
        uint2 cellIndex = uint2(floor(menuCoord / float2(BASE_CHAR_SIZE)));

        if (flipX) cellIndex.x = line.charCount - 1 - cellIndex.x;

        if (cellIndex.y == line.lineIndex) {
            if (cellIndex.x > 127) return; // max line length is 128 chars
            if (cellIndex.x >= line.charCount) return;

            uint2 char = line.chars[cellIndex.x];

            uint2 bitCoords = uint2(floor(cellCoord * 8.0)); // get coordinates into the bitmap for the character
            bitCoords.y = 7 - bitCoords.y;
            if (flipX) bitCoords.x = 7 - bitCoords.x;

            uint bitIndex = bitCoords.x + bitCoords.y * 8; // linearize coordinates into index

            // color = float4(select(all(char == uint2(0)), 1.0, 0.0), 0.0, 0.0, 1.0);
            // return;

            uint bit;

            if (bitIndex > 31) {
                bit = bitfieldExtract(char.y, bitIndex - 32, 1);
            } else {
                bit = bitfieldExtract(char.x, bitIndex, 1);
            }

            if (bit == 1) {
                color.rgb = FOREGROUND_COLOR;
                color.a = FOREGROUND_ALPHA;
            } else {
                color.rgb = BACKGROUND_COLOR;
                color.a = BACKGROUND_ALPHA;
            }
        }
    }

    // if (cellIndex.x > 127) return; // max line length is 128 chars

    // TextLine line = lines[cellIndex.y];

    // if (cellIndex.x > line.charCount - 1) return;

    // uint2 char = line.chars[cellIndex.x];

    // uint2 bitCoords = uint2(floor(cellCoord * 8.0)); // get coordinates into the bitmap for the character
    // uint bitIndex = bitCoords.x + bitCoords.y * 8; // linearize coordinates into index

    // color = float4(char, 0.0, 1.0);
    // return;

    // uint bit;

    // if (bitIndex > 31) {
    //     bit = bitfieldExtract(char.y, bit - 32, 1);
    // } else {
    //     bit = bitfieldExtract(char.x, bit, 1);
    // }

    // if (bit == 1) {
    //     color.rgb = FOREGROUND_COLOR;
    //     color.a = FOREGROUND_ALPHA;
    // } else {
    //     color.rgb = BACKGROUND_COLOR;
    //     color.a = BACKGROUND_ALPHA;
    // }
}

[[numthreads(8, 8, 1)]]
[[shader("compute")]]
void compute(
    uniform ParameterBlock<Screen> screen,

    uniform ParameterBlock<PostTextureState> textures,
    uniform ParameterBlock<Menu> menu,
    
    uint3 globalInvocationId: SV_DispatchThreadID,
    uint3 localInvocationId: SV_GroupThreadID,
    uint localInvocationIndex: SV_GroupIndex,
) {
    if (globalInvocationId.x >= screen.view.width || globalInvocationId.y >= screen.view.height) {
        return;
    }
    
    float4 menuColor = float4(0.0);

    renderMenu(menuColor, globalInvocationId.xy, screen.view.width, menu.data, menu.left, menu.data.leftLineCount);
    renderMenu(menuColor, globalInvocationId.xy, screen.view.width, menu.data, menu.right, menu.data.rightLineCount, true);

    float4 inputColor = textures.inputTexture.Load(int3(globalInvocationId.xy, 0));

    float4 finalColor = lerp(inputColor, menuColor, menuColor.a);
    textures.outputTexture.Store(globalInvocationId.xy, finalColor);
}