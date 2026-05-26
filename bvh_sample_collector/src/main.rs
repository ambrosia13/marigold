use std::{error::Error, path::Path, sync::atomic::AtomicU32};

use bvh::{BoundingVolumeHierarchy, BvhSettings};
use gltf_loading::GltfScenes;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefMutIterator, ParallelIterator,
};

fn main() -> Result<(), Box<dyn Error>> {
    // example format: ./cmd ../meshes ./bvh_snapshots parallelize_whatever 15
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mesh_dir = &args[0];
    let output_dir = &args[1];
    let generation_name = &args[2];
    let num_samples = args[3].parse::<u32>()?;

    let mesh_dir_path = Path::new(mesh_dir);

    for entry in std::fs::read_dir(mesh_dir_path)? {
        let entry = entry?;

        let mesh_name = entry.file_name();
        let mesh_name = mesh_name.to_string_lossy();

        let gltf_path = entry.path();

        println!("loading gltf scene {}...", mesh_name);

        let gltf = GltfScenes::load(gltf_path);
        let (meshes, _) = gltf.into_meshes_and_scenes();

        println!("finished loading gltf scene {}!", mesh_name);

        let mesh_name_str = &mesh_name;

        let sample_index = AtomicU32::new(1);

        // build bvhs for each scene N times
        // we can do it in parallel per sample to speed up, but incur an extra cost
        // because the mesh needs to be cloned
        (0..num_samples).into_par_iter().for_each(|_| {
            let mut meshes = meshes.clone();

            meshes.par_iter_mut().enumerate().for_each(|(index, mesh)| {
                let settings = BvhSettings {
                    name: &format!("{}_{}", mesh_name_str, index),
                    bounds: Some(mesh.bounds),
                    max_depth: 24, // use depth = 24 for blas max depth
                    profiling_info: true,
                    profiling_info_directory: Some(&Path::new(output_dir).join(generation_name)),
                };

                // build the bvh but do nothing with it
                let _ = BoundingVolumeHierarchy::<1, 1>::new(
                    &mut mesh.triangles,
                    &mesh.vertices,
                    settings,
                );
            });

            println!(
                "\tsample {} done for {}!",
                sample_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                mesh_name_str
            );
        });
    }

    Ok(())
}
