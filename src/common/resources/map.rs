use bevy::{
    asset::RenderAssetUsages, 
    prelude::*, 
    render::{
        mesh::{Indices, PrimitiveTopology}, 
        primitives::Aabb
    },
};

use qrz::{self, Convert, Qrz};

use crate::common::components::entity_type::*;

#[derive(Clone, Resource)]
pub struct Map(qrz::Map<EntityType>);

impl Map {
    pub fn new(map: qrz::Map<EntityType>) -> Map {
        Map(map)
    }

    pub fn regenerate_mesh(&self) -> (Mesh,Aabb) {
        let mut verts:Vec<Vec3> = Vec::new();
        let mut norms:Vec<Vec3> = Vec::new();
        let mut last_qrz:Option<Qrz> = None;
        let mut skip_sw = false;
        let mut west_skirt_verts: Vec<Vec3> = Vec::new();
        let mut west_skirt_norms: Vec<Vec3> = Vec::new();
        let (mut min, mut max) = (Vec3::new(f32::MAX, f32::MAX, f32::MAX), Vec3::new(f32::MIN, f32::MIN, f32::MIN));

        let map = self.0.clone();
        map.clone().into_iter().for_each(|tile| {
            // next
            // 0 - 0
            // 1 - 5
            // 2 (012) - 6 - 056            | 6  - 056          | 
            // 3 (213) - 4 - 654            | 4  - 654          | 
            // 4 (234) - 3 - 643   - end    | 3  - 643          | 
            // 5 (435) - 0 - 3'4'0 - skirt  | 3  - 343    - end | 
            // 6 (456) - 1 - 3'01  - skirt  | 0^ - 3'3'0^       | 
            // 7 (657) - 6 - 106            | 0^ - 0^3'0^       | 
            // 8 (678) - 2 - 162            | 0  - 0^0^0        | 
            // 9 (879) - 3 - 263   - end    | 0  - 00^0         |
            //10 (890) - 0 - 2'3'0 - skirt  | 0  - 000          |
            //11 (091) - 5 - 03'5  - skirt  | 5  - 005          |
            //12 (012) - 6 - 056            | 6  - 056          |
            
            let it_qrz = tile.0;
            let it_vrt = self.0.vertices(it_qrz);

            if let Some(last_qrz) = last_qrz {
                // if new column
                if last_qrz.q*2+last_qrz.r != it_qrz.q*2+it_qrz.r {
                    // add skirts
                    verts.append(&mut west_skirt_verts);
                    norms.append(&mut west_skirt_norms);

                    // update bounding box
                    let last_vrt = self.0.vertices(last_qrz);
                    min = Vec3::min(min, it_vrt[6]);
                    min = Vec3::min(min, last_vrt[6]);
                    max = Vec3::max(max, it_vrt[6]);
                    max = Vec3::max(max, last_vrt[6]);
                }
            }

            let sw_qrz = self.find(it_qrz + Qrz{q:0,r:0,z:5} + qrz::DIRECTIONS[1], -10);
            let sw_vrt = 
                if sw_qrz.is_none() { None }
                else { Some(self.0.vertices(sw_qrz.unwrap().0)) };

            if skip_sw {
                let last_vrt = self.0.vertices(last_qrz.unwrap());
                let last_vrt_underover = Vec3::new(last_vrt[3].x, it_vrt[0].y, last_vrt[3].z);
                verts.extend([ last_vrt_underover, last_vrt_underover, it_vrt[0], it_vrt[0] ]);
                norms.extend([ Vec3::new(0., 1., 0.); 4 ]);
                skip_sw = false;
            }
            
            verts.extend([ it_vrt[0], it_vrt[5], it_vrt[6], it_vrt[4], it_vrt[3] ]);
            norms.extend([ Vec3::new(0., 1., 0.); 5 ]);

            if let Some(sw_vrt) = sw_vrt {
                verts.extend([ sw_vrt[0], sw_vrt[1], sw_vrt[6], sw_vrt[2], sw_vrt[3]]);
                norms.extend([ Vec3::new(0., 1., 0.); 5 ]);
            } else {
                verts.extend([ it_vrt[3] ]); 
                norms.extend([ Vec3::new(0., 1., 0.); 1 ]);
                skip_sw = true;
            }
            // next
            // 0 - 5
            // 1 - 1'
            // 2 (012) - 4  - 51'4
            // 3 (213) - 2' - 41'2'
            // 4 (234) - 4  - 42'4
            // 5 (435) - 4  - 42'4 - end
            // 6 (456) - 5v - 4'4'5v
            // 7 (657) - 5v  - 5v4'5v
            // 8 (678) - 5  - 5v5v5
            // 9 (879) - 1' - 55v1'
            //10 (890) - 4  - 51'4
            let we_qrz = self.find(it_qrz + Qrz{q:0,r:0,z:5} + qrz::DIRECTIONS[0], -10)
                .unwrap_or((it_qrz + qrz::DIRECTIONS[0], EntityType::Decorator(default()))).0;
            let we_vrt = self.0.vertices(we_qrz);
            
            if let Some(last_qrz) = last_qrz {
                let last_vrt = self.0.vertices(last_qrz);
                let last_vrt_underover = Vec3::new(it_vrt[5].x, last_vrt[4].y, it_vrt[5].z);
                west_skirt_verts.extend([ last_vrt_underover, last_vrt_underover ]);
                west_skirt_norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
            }
            west_skirt_verts.extend([ it_vrt[5], we_vrt[1], it_vrt[4], we_vrt[2], it_vrt[4], it_vrt[4] ]);
            west_skirt_norms.extend([ Vec3::new(0., 1., 0.); 6 ]);
            
            last_qrz = Some(it_qrz);
        });

        let len = verts.clone().len() as u32;
        (
            Mesh::new(PrimitiveTopology::TriangleStrip, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..len).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_indices(Indices::U32((0..len).collect())),
            Aabb::from_min_max(min, max),
        )
    }

    pub fn find(&self, qrz: Qrz, dist: i8) -> Option<(Qrz, EntityType)> { self.0.find(qrz, dist) }
    pub fn get(&self, qrz: Qrz) -> Option<&EntityType> { self.0.get(qrz) }
    pub fn insert(&mut self, qrz: Qrz, obj: EntityType) { self.0.insert(qrz, obj); }
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
