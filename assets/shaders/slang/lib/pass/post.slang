public struct PostTextureState {
    public SamplerState inputSampler;

    [format("r11f_g11f_b10f")]
    public Texture2D inputTexture;

    [format("r11f_g11f_b10f")]
    public RWTexture2D outputTexture;
}