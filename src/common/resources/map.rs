use bevy::{
    asset::RenderAssetUsages, 
    prelude::*, 
    render::mesh::{Indices, PrimitiveTopology}
};
use qrz::{self, Convert, Qrz};

#[derive(Clone, Resource)]
pub struct Map(qrz::Map<Entity>);

impl Map {
    pub fn new(map: qrz::Map<Entity>) -> Map {
        Map(map)
    }

    pub fn regenerate_mesh(&self) -> Mesh {
        let mut verts:Vec<Vec3> = Vec::new();
        let mut last_qrz:Option<Qrz> = None;
        self.0.clone().into_iter().for_each(|tile| {
            let curr_qrz = tile.0;
            let next_qrz = tile.0 + Qrz { q: -1, r: 1, z: 0 };
            let curr_vrt = self.0.vertices(curr_qrz);
            let next_vrt = self.0.vertices(next_qrz);
            // debug!("qrz: {:?} = {:?} -> {:?}", tile.0, tile.0.into_doublewidth(), curr_vrt);
            // if last_qrz.is_some() && last_qrz.unwrap().r != curr_qrz.r - 2 {
                verts.extend([ curr_vrt[0], curr_vrt[1] ]);
            // }
            verts.extend([ self.0.convert(curr_qrz), curr_vrt[2], curr_vrt[3] ].iter());
            verts.extend([ self.0.convert(next_qrz), next_vrt[4], next_vrt[3] ].iter());
            last_qrz = Some(curr_qrz);
        });

        let len = verts.clone().len() as u32;
        Mesh::new(PrimitiveTopology::TriangleStrip, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..len).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, (0..len).map(|_| [0., 1., 0.]).collect::<Vec<[f32; 3]>>())
            .with_inserted_indices(Indices::U32((0..len).collect()))
    }

    pub fn find(&self, qrz: Qrz, dist: i8) -> Option<(Qrz, Entity)> { self.0.find(qrz, dist) }
    pub fn get(&self, qrz: Qrz) -> Option<&Entity> { self.0.get(qrz) }
    pub fn insert(&mut self, qrz: Qrz, obj: Entity) { self.0.insert(qrz, obj); }
    pub fn radius(&self) -> f32 { self.0.radius() }
}

impl Convert<Qrz, Vec3> for Map {
    fn convert(&self, it: Qrz) -> Vec3 {
        self.0.convert(it)
    }
}

impl Convert<Vec3, Qrz> for Map {
    fn convert(&self, it: Vec3) -> Qrz {
        self.0.convert(it)
    }
}

// next
// 0 - 0
// 1 - 1
// 2 - c - 01c (012)
// 3 - 2 - c12 (213)
// 4 - 3 - c23 (234) - end
// 5 - c - 32c (435)
// 6 - 4 - 3c4 (456)
// 7 - 3 - 4c3 (657) - end

// 8 - 0 - 430 (678) - cull
// 9 - 1 - 031 (879) - cull
//10 - c - 01c (890)