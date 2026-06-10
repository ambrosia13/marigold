namespace bindless {
    // format is rgba8 since that matches most image formats, like png and jpeg
    // used for albedo, normal, specular, etc., alpha may be used for culling
    [vk_image_format("rgba8")]
    [vk_binding(0, 100)]
    public Texture2D<float>[] materialTextures;

    // generic float lookup tables, input depends on the context (e.g. SPD expects wavelength range),
    // output is always a float. used for SPDs, coefficients, density-by-radius, etc.
    [vk_image_format("r32f")]
    [vk_binding(1, 100)]
    public Texture1D<float>[] lookupTables;

    // will probably just be simple linear sampler for all cases, but can be swapped out later
    [vk_binding(2, 100)]
    public SamplerState currentSampler;
}
