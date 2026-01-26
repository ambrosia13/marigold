pub mod background;
pub mod bake;
pub mod display;
pub mod geometry;
pub mod post;

/*
** passes **

compile-time:
    - bake 0...n
        - atmosphere LUTs

runtime:
    - background
        - atmosphere or cubemap
    - geometry
        - path trace, debug render, BVH debug render
    - post 0...n
        - bloom, tone map, exposure, menu
    - display (is render pass, writes to surface texture, simple blit?)
*/
