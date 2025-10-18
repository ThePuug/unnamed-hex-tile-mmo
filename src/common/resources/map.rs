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

    /// Generate vertices for a hex tile with slopes toward neighbors
    /// Each edge adjusts by 0.5 * rise, but vertices on multiple edges take the max adjustment
    pub fn vertices_with_slopes(&self, qrz: Qrz) -> Vec<Vec3> {
        let mut verts = self.0.vertices(qrz);
        let rise = self.0.rise();
        
        // Track adjustments per vertex to apply only the maximum
        let mut vertex_adjustments: [Vec<f32>; 6] = Default::default();
        
        // Map of direction index to the two vertices on that edge
        let direction_to_vertices = [
            (4, 5), // Dir 0: West edge has vertices SW(4) and NW(5)
            (3, 4), // Dir 1: SW edge has vertices South(3) and SW(4)
            (2, 3), // Dir 2: SE edge has vertices SE(2) and South(3)
            (1, 2), // Dir 3: East edge has vertices NE(1) and SE(2)
            (0, 1), // Dir 4: NE edge has vertices North(0) and NE(1)
            (5, 0), // Dir 5: NW edge has vertices NW(5) and North(0)
        ];

        // Process each edge independently based on its neighbor
        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = qrz + *direction;
            
            // Try to find the neighbor tile across this edge (search both up and down)
            let found_neighbor = self.find(neighbor_qrz, -10)
                .or_else(|| self.find(neighbor_qrz, 10));
            
            if let Some((actual_neighbor_qrz, _)) = found_neighbor {
                // Calculate elevation difference
                let elevation_diff = actual_neighbor_qrz.z - qrz.z;
                
                // Adjust by 0.5 * rise to meet neighbor halfway
                let adjustment = if elevation_diff > 0 {
                    rise * 0.5  // Neighbor is higher, slope up
                } else if elevation_diff < 0 {
                    rise * -0.5  // Neighbor is lower, slope down
                } else {
                    0.0  // Same level, no slope
                };

                // Record adjustment for vertices on this edge
                if adjustment != 0.0 {
                    let (v1, v2) = direction_to_vertices[dir_idx];
                    vertex_adjustments[v1].push(adjustment);
                    vertex_adjustments[v2].push(adjustment);
                }
            }
        }

        // Apply the maximum absolute adjustment to each vertex
        for (i, adjustments) in vertex_adjustments.iter().enumerate() {
            if !adjustments.is_empty() {
                let max_adj = adjustments.iter()
                    .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
                    .copied().unwrap();
                verts[i].y += max_adj;
            }
        }

        verts
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
            let it_qrz = tile.0;
            let it_vrt = self.vertices_with_slopes(it_qrz);

            if let Some(last_qrz) = last_qrz {
                // if new column
                if last_qrz.q*2+last_qrz.r != it_qrz.q*2+it_qrz.r {
                    // add skirts
                    verts.append(&mut west_skirt_verts);
                    norms.append(&mut west_skirt_norms);

                    // update bounding box
                    let last_vrt = self.vertices_with_slopes(last_qrz);
                    min = Vec3::min(min, it_vrt[6]);
                    min = Vec3::min(min, last_vrt[6]);
                    max = Vec3::max(max, it_vrt[6]);
                    max = Vec3::max(max, last_vrt[6]);
                }
            }

            let sw_result = self.find(it_qrz + Qrz{q:0,r:0,z:5} + qrz::DIRECTIONS[1], -10);
            let sw_vrt = sw_result.map(|(qrz, _)| self.vertices_with_slopes(qrz));

            if skip_sw {
                let last_vrt = self.vertices_with_slopes(last_qrz.unwrap());
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

            let we_result = self.find(it_qrz + Qrz{q:0,r:0,z:5} + qrz::DIRECTIONS[0], -10);
            let we_qrz = we_result.unwrap_or((it_qrz + qrz::DIRECTIONS[0], EntityType::Decorator(default()))).0;
            // Only use sloped vertices if the tile actually exists in the map
            let mut we_vrt = if we_result.is_some() {
                self.vertices_with_slopes(we_qrz)
            } else {
                self.0.vertices(we_qrz)
            };
            
            // If west neighbor is fake, match its East edge vertices to current tile's West edge
            if we_result.is_none() {
                we_vrt[1].y = it_vrt[5].y;  // NE of west neighbor = NW of current tile  
                we_vrt[2].y = it_vrt[4].y;  // SE of west neighbor = SW of current tile
            }
            
            if let Some(last_qrz) = last_qrz {
                let last_vrt = self.vertices_with_slopes(last_qrz);
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
    pub fn neighbors(&self, qrz: Qrz) -> Vec<(Qrz, EntityType)> { self.0.neighbors(qrz) }
    pub fn iter_tiles(&self) -> impl Iterator<Item = (Qrz, EntityType)> + '_ {
        self.0.clone().into_iter()
    }
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
