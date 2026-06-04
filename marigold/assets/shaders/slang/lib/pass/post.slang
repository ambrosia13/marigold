public struct PostTextureState {
    public SamplerState inputSampler;

    [vk_image_format("r11f_g11f_b10f")]
    public Texture2D inputTexture;

    [vk_image_format("r11f_g11f_b10f")]
    public RWTexture2D outputTexture;
}
