use bevy::{
    asset::RenderAssetUsages, 
    prelude::*, 
    render::mesh::{Indices, PrimitiveTopology}
};

use qrz::{self, Convert, Qrz};
use crate::common::components::heading::Heading;

#[derive(Clone, Resource)]
pub struct Map(qrz::Map<Entity>);

impl Map {
    pub fn new(map: qrz::Map<Entity>) -> Map {
        Map(map)
    }

    pub fn regenerate_mesh(&self) -> Mesh {
        let mut verts:Vec<Vec3> = Vec::new();
        let mut norms:Vec<Vec3> = Vec::new();
        let mut last_qrz:Option<Qrz> = None;
        let mut skip_next = false;
        self.0.clone().into_iter().for_each(|tile| {
            let curr_qrz = tile.0;
            let next_qrz = self.find(curr_qrz + qrz::DIRECTIONS[5],5);
            let curr_vrt = self.0.vertices(curr_qrz);
            let next_vrt = 
                if next_qrz.is_none() { None }
                else { Some(self.0.vertices(next_qrz.unwrap().0)) };
            if skip_next {
                verts.extend([ curr_vrt[0] ]);
                norms.extend([ Vec3::new(0., 1., 0.) ]);
                skip_next = false;
            }
            // if let Some(last_qrz) = last_qrz {
            //     let last_col = 2 * last_qrz.q + last_qrz.r;
            //     let curr_col = 2 * curr_qrz.q + curr_qrz.r;
            //     if last_col != curr_col {
            //         verts.extend([ self.0.vertices(last_qrz + qrz::DIRECTIONS[2])[3], curr_vrt[0] ]);
            //         norms.extend([ Vec3::new(0., 0., 1.); 2 ]);
            //     }
                verts.extend([ curr_vrt[0], curr_vrt[1] ]);
                norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
            // }
            verts.extend([ curr_vrt[6], curr_vrt[2], curr_vrt[3] ]);
            norms.extend([ Vec3::new(0., 1., 0.); 3 ]);
            if let Some(next_vrt) = next_vrt {
                verts.extend([ next_vrt[0], next_vrt[5]]);
                norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
                verts.extend([ next_vrt[6], next_vrt[4], next_vrt[3] ]);
                norms.extend([ Vec3::new(0., 1., 0.); 3 ]);
            } else {
                verts.extend([ curr_vrt[3], curr_vrt[3] ]); 
                norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
                skip_next = true;
            }
            last_qrz = Some(curr_qrz);
        });

        let len = verts.clone().len() as u32;
        Mesh::new(PrimitiveTopology::TriangleStrip, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..len).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
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
// 2 (012) - c - 01c
// 3 (213) - 2 - c12
// 4 (234) - 3 - c23   - end
// 5 (435) - 0 - 3'2'0 - cull,skirt
// 6 (456) - 5 - 3'05  - cull,skirt
// 7 (657) - c - 50c
// 8 (678) - 4 - 5c4
// 9 (879) - 3 - 4c3   - end   
//10 (890) - 0 - 4'3'0 - cull,skirt 
//11 (091) - 1 - 03'1  - cull,skirt
//12 (012) - c - 01c   