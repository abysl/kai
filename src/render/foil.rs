use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub type FoilMaterial = ExtendedMaterial<StandardMaterial, FoilExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct FoilExtension {
    #[uniform(100)]
    pub strength: f32,
    #[uniform(100)]
    pub frequency: f32,
    #[uniform(100)]
    pub uv_drift: f32,
    #[uniform(100)]
    pub cell_density: f32,
    #[uniform(100)]
    pub spark_strength: f32,
    #[uniform(100)]
    pub _pad0: f32,
    #[uniform(100)]
    pub _pad1: f32,
    #[uniform(100)]
    pub _pad2: f32,
}

impl Default for FoilExtension {
    fn default() -> Self {
        Self {
            strength: 0.8,
            frequency: 3.0,
            uv_drift: 0.35,
            cell_density: 16.0,
            spark_strength: 1.2,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
        }
    }
}

impl MaterialExtension for FoilExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/foil.wgsl".into()
    }
}
