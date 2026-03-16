#[derive(Clone, Copy)]
pub enum GeometryId {
    Sphere,
    Aabb,
    Quad,
    Mesh(u32),
}

impl GeometryId {
    pub fn encode(self) -> u16 {
        match self {
            GeometryId::Sphere => u16::MAX,
            GeometryId::Aabb => u16::MAX - 1,
            GeometryId::Quad => u16::MAX - 2,
            GeometryId::Mesh(i) => i as u16,
        }
    }
}
