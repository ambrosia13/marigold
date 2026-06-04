public struct GeometryTextures {
    [vk_image_format("rgba32f")]
    public RWTexture2D current;
    public Texture2D previous;
}
