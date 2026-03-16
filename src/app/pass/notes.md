# pass order

- compile-time:
    - `bake` 0...n
        - atmosphere LUTs

- runtime:
    - `background`
        - atmosphere or cubemap
    - `geometry`
        - path trace, debug render, or BVH debug render
    - `post` 0...n
        - bloom, tone map, exposure, gamma correct, menu
    - `display` (is render pass, writes to surface texture, simple blit?)

# code structure

- "single" passes such as `geometry` and `background` are resources that represent the pass, with potentialy interchangable pipelines
- "stackable" passes such as `bake` and `post` are raw structs, but there is a resource that schedules each one; e.g. `PostSchedule` and `BakeSchedule`. the passes are created and ordered in the constructors of each of these schedule structs, and the schedule runs them each frame (can use a &mut world to make parameters easier, since can't run in parallel with anything else either way)