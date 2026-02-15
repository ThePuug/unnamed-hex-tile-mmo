use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};
use bevy_camera::primitives::Aabb;
use bevy_mesh::Indices;

use qrz::{self, Convert, Qrz};

use crate::common::{
    chunk::{ChunkId, loc_to_chunk},
    components::entity_type::*,
};

#[derive(Clone, Resource)]
pub struct Map(qrz::Map<EntityType>);

impl Map {
    pub fn new(map: qrz::Map<EntityType>) -> Map {
        Map(map)
    }

    /// Get the vertical rise per Z level from the underlying map
    pub fn rise(&self) -> f32 {
        self.0.rise()
    }

    /// Generate vertices for a hex tile with slopes toward neighbors
    /// Returns (vertices, vertex_colors) - combined to avoid duplicate neighbor searches
    /// If apply_slopes is false, vertices remain flat at their natural height
    /// Calculate height-based color tint for a given elevation
    fn height_color_tint(&self, elevation: i16) -> [f32; 4] {
        // Base grass color (dark greenish)
        let base_color = [0.04, 0.09, 0.04];

        // Apply elevation-based tinting
        // Lower elevations (valleys): darker, more saturated green
        // Higher elevations (peaks): lighter, with hints of brown/gray (suggesting rocky terrain)
        let elevation_factor = (elevation as f32) / 15.0; // Normalize to roughly 0-1 range

        // Darker at low elevations, lighter at high elevations
        let brightness_mult = 1.0 + (elevation_factor * 2.0).clamp(0.0, 2.0);

        // At high elevations, add brown/gray tint (reduce green, add red)
        let high_elevation_tint = elevation_factor.clamp(0.0, 1.0);

        [
            (base_color[0] * brightness_mult + high_elevation_tint * 0.15).clamp(0.0, 1.0), // Red increases with height
            (base_color[1] * brightness_mult * (1.0 - high_elevation_tint * 0.3)).clamp(0.0, 1.0), // Green slightly decreases
            (base_color[2] * brightness_mult).clamp(0.0, 1.0), // Blue stays similar
            1.0, // Alpha
        ]
    }

    pub fn vertices_and_colors_with_slopes(&self, qrz: Qrz, apply_slopes: bool) -> (Vec<Vec3>, Vec<[f32; 4]>) {
        let mut verts = self.0.vertices(qrz);
        let rise = self.0.rise();

        // Use height-based color instead of fixed grass color
        let grass_color = self.height_color_tint(qrz.z);
        // Stone cliff color (lighter gray-brown for contrast)
        let cliff_color = [0.35, 0.32, 0.28, 1.0];
        let mut colors = vec![grass_color; 6];

        // Calculate ambient occlusion for each vertex
        // Vertices surrounded by higher neighbors should be darkened
        let mut ao_factors = [1.0f32; 6]; // 1.0 = no darkening, lower values = darker

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
        // Do neighbor search once and use for both slopes and cliff detection
        for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
            let neighbor_qrz = qrz + *direction;
            
            // Try to find the neighbor tile across this edge (search both up and down)
            let found_neighbor = self.find(neighbor_qrz + Qrz{q:0,r:0,z:30}, -60)
                .or_else(|| self.find(neighbor_qrz + Qrz{q:0,r:0,z:-30}, 60));
            
            if let Some((actual_neighbor_qrz, _)) = found_neighbor {
                // Calculate elevation difference
                let elevation_diff = actual_neighbor_qrz.z - qrz.z;

                // Ambient occlusion: darken vertices next to higher neighbors
                if elevation_diff > 0 {
                    let (v1, v2) = direction_to_vertices[dir_idx];
                    // Darken based on height difference (more height = more darkening)
                    let ao_amount = (elevation_diff as f32 / 10.0).min(0.3); // Max 30% darkening per neighbor
                    ao_factors[v1] *= 1.0 - ao_amount;
                    ao_factors[v2] *= 1.0 - ao_amount;
                }

                // Check if this is a cliff edge (elevation difference > 1)
                let is_cliff = elevation_diff.abs() > 1;
                
                // Slope calculation:
                // - Allow slopes on both sides of cliffs (top slopes down, bottom slopes up)
                // - This creates more gradual cliff faces
                let adjustment = if is_cliff && elevation_diff > 1 {
                    rise * 0.5  // Upward cliff: slope up toward higher neighbor
                } else if is_cliff && elevation_diff < -1 {
                    rise * -0.5  // Downward cliff: slope down toward lower neighbor
                } else if elevation_diff > 0 {
                    rise * 0.5  // Gradual up: slope up
                } else if elevation_diff < 0 {
                    rise * -0.5  // Gradual down: slope down
                } else {
                    0.0  // Same level, no slope
                };

                // Record adjustment for vertices on this edge
                if adjustment != 0.0 {
                    let (v1, v2) = direction_to_vertices[dir_idx];
                    vertex_adjustments[v1].push(adjustment);
                    vertex_adjustments[v2].push(adjustment);
                }
                
                // Darken vertices at the BOTTOM of cliffs (looking up at higher neighbor)
                // Keep vertices at the TOP of cliffs (looking down) at normal color
                if is_cliff && elevation_diff > 1 {
                    // This is the bottom of a cliff - darken vertices
                    let (v1, v2) = direction_to_vertices[dir_idx];
                    colors[v1] = cliff_color;
                    colors[v2] = cliff_color;
                }
                // Don't darken vertices at the top of cliffs (elevation_diff < -1)
            }
        }

        // Apply the maximum absolute adjustment to each vertex
        // Now we allow slopes on both sides of cliffs for more gradual transitions
        if apply_slopes {
            for (i, adjustments) in vertex_adjustments.iter().enumerate() {
                if adjustments.is_empty() {
                    continue;
                }

                // Apply the adjustment with the largest absolute value
                let max_adj = adjustments.iter()
                    .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
                    .copied().unwrap();
                verts[i].y += max_adj;
            }
        }

        // Apply ambient occlusion to all vertex colors
        for i in 0..6 {
            colors[i][0] *= ao_factors[i];
            colors[i][1] *= ao_factors[i];
            colors[i][2] *= ao_factors[i];
            // Alpha stays at 1.0
        }

        (verts, colors)
    }

    /// Calculate smooth normal for a vertex by averaging face normals of adjacent triangles
    /// This version considers neighboring hexes for truly smooth lighting
    fn calculate_vertex_normal(&self, _qrz: Qrz, _vertex_idx: usize, _verts: &[Vec3], _apply_slopes: bool) -> Vec3 {
        // For smooth terrain appearance, use flat upward normals
        // This prevents each hex from showing as a distinct bump
        Vec3::new(0., 1., 0.)
    }

    /// Generate a mesh for a single chunk using TriangleList topology
    /// This enables independent chunk rendering and better GPU cache locality
    pub fn generate_chunk_mesh(&self, chunk_id: ChunkId, apply_slopes: bool) -> (Mesh, Aabb) {
        let mut verts: Vec<Vec3> = Vec::new();
        let mut norms: Vec<Vec3> = Vec::new();
        let mut colors: Vec<[f32; 4]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let mut tile_count = 0;

        // Filter tiles to only those in this chunk
        for (tile_qrz, _) in self.0.clone().into_iter() {
            if loc_to_chunk(tile_qrz) != chunk_id {
                continue;
            }

            tile_count += 1;

            // Get RAW vertices (without slope adjustments to avoid gaps at boundaries)
            // Only apply slopes if explicitly enabled AND we're okay with gaps
            let raw_verts = self.0.vertices(tile_qrz);
            let (slope_verts, tile_colors) = if apply_slopes {
                self.vertices_and_colors_with_slopes(tile_qrz, true)
            } else {
                let colors = vec![self.height_color_tint(tile_qrz.z); 6];
                (raw_verts.clone(), colors)
            };

            // Use raw vertices for edge vertices to ensure adjacent hexes align perfectly
            // Use slope-adjusted vertices only for height, not position
            let tile_verts: Vec<Vec3> = raw_verts.iter().enumerate().map(|(i, &raw_pos)| {
                if apply_slopes && i < 6 {
                    // Keep X/Z from raw, but use Y from slope-adjusted
                    Vec3::new(raw_pos.x, slope_verts[i].y, raw_pos.z)
                } else {
                    raw_pos
                }
            }).collect();

            // Base index for this tile's vertices
            let base_idx = verts.len() as u32;

            // Center vertex (index 6)
            let center_pos = tile_verts[6];
            let center_color = self.height_color_tint(tile_qrz.z);
            let center_normal = Vec3::new(0., 1., 0.);

            verts.push(center_pos);
            colors.push(center_color);
            norms.push(center_normal);

            // Outer vertices (indices 0-5: N, NE, SE, S, SW, NW)
            for i in 0..6 {
                verts.push(tile_verts[i]);
                colors.push(tile_colors[i]);
                norms.push(self.calculate_vertex_normal(tile_qrz, i, &tile_verts, apply_slopes));
            }

            // Generate 6 triangles for the hex top surface (TriangleList)
            // Hex vertices are ordered clockwise (N, NE, SE, S, SW, NW)
            // Reverse winding to counter-clockwise for Bevy's backface culling
            for i in 0..6 {
                let v1 = base_idx + 1 + i;
                let v2 = base_idx + 1 + ((i + 1) % 6);
                indices.extend([base_idx, v2, v1]); // Reversed winding: [center, v2, v1]
            }

            // Add vertical skirt geometry for edges with elevation changes
            for (dir_idx, direction) in qrz::DIRECTIONS.iter().enumerate() {
                let neighbor_qrz = tile_qrz + *direction;

                // Find neighbor at different elevation (search up/down)
                let found_neighbor = self.find(neighbor_qrz + Qrz{q:0,r:0,z:30}, -60)
                    .or_else(|| self.find(neighbor_qrz + Qrz{q:0,r:0,z:-30}, 60));

                if let Some((actual_neighbor_qrz, _)) = found_neighbor {
                    let elevation_diff = actual_neighbor_qrz.z - tile_qrz.z;

                    // Only add skirt if there's a cliff drop (elevation difference >= 2)
                    // Each hex is in exactly one chunk, so no duplicates - we render skirts
                    // for all our hexes regardless of where the neighbor is
                    if elevation_diff >= -1 {
                        continue;
                    }

                    // Get neighbor vertices
                    let (neighbor_verts, neighbor_colors) = self.vertices_and_colors_with_slopes(actual_neighbor_qrz, apply_slopes);

                    // Map direction to edge vertices
                    // dir_idx 0 (West): current hex SW(4) and NW(5), neighbor NE(1) and SE(2)
                    // dir_idx 1 (SW): current hex S(3) and SW(4), neighbor N(0) and NE(1)
                    // dir_idx 2 (SE): current hex SE(2) and S(3), neighbor NW(5) and N(0)
                    // dir_idx 3 (East): current hex NE(1) and SE(2), neighbor SW(4) and NW(5)
                    // dir_idx 4 (NE): current hex N(0) and NE(1), neighbor S(3) and SW(4)
                    // dir_idx 5 (NW): current hex NW(5) and N(0), neighbor SE(2) and S(3)
                    let (curr_v1_idx, curr_v2_idx, neighbor_v1_idx, neighbor_v2_idx) = match dir_idx {
                        0 => (4, 5, 1, 2), // West
                        1 => (3, 4, 0, 1), // SW
                        2 => (2, 3, 5, 0), // SE
                        3 => (1, 2, 4, 5), // East
                        4 => (0, 1, 3, 4), // NE
                        5 => (5, 0, 2, 3), // NW
                        _ => continue,
                    };

                    let curr_v1 = tile_verts[curr_v1_idx];
                    let curr_v2 = tile_verts[curr_v2_idx];
                    let neighbor_v1 = neighbor_verts[neighbor_v1_idx];
                    let neighbor_v2 = neighbor_verts[neighbor_v2_idx];

                    let curr_c1 = tile_colors[curr_v1_idx];
                    let curr_c2 = tile_colors[curr_v2_idx];
                    let neighbor_c1 = neighbor_colors[neighbor_v1_idx];
                    let neighbor_c2 = neighbor_colors[neighbor_v2_idx];

                    // Add 4 vertices for the vertical quad
                    let skirt_base = verts.len() as u32;
                    verts.extend([curr_v1, curr_v2, neighbor_v2, neighbor_v1]);
                    colors.extend([curr_c1, curr_c2, neighbor_c2, neighbor_c1]);

                    // Normal pointing outward from the edge
                    let edge_normal = Vec3::new(0., 0., 1.); // Simplified, could calculate actual normal
                    norms.extend([edge_normal; 4]);

                    // Two triangles forming the vertical quad (counter-clockwise winding from outside)
                    // Vertices: 0=bottom-left, 1=top-left, 2=bottom-right, 3=top-right
                    // Triangle 1: [0, 1, 3] counter-clockwise
                    // Triangle 2: [0, 3, 2] counter-clockwise
                    indices.extend([skirt_base, skirt_base + 1, skirt_base + 3]);
                    indices.extend([skirt_base, skirt_base + 3, skirt_base + 2]);
                }
            }
        }

        // Compute AABB from chunk vertices only
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for vert in &verts {
            min = Vec3::min(min, *vert);
            max = Vec3::max(max, *vert);
        }

        println!("Chunk mesh: chunk_id=({},{}), {} tiles, {} vertices, {} indices, AABB: {:?} to {:?}",
                 chunk_id.0, chunk_id.1, tile_count, verts.len(), indices.len(), min, max);

        let vert_count = verts.len();

        (
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD
            )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..vert_count).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
                .with_inserted_indices(Indices::U32(indices)),
            Aabb::from_min_max(min, max),
        )
    }

    pub fn regenerate_mesh(&self, apply_slopes: bool) -> (Mesh,Aabb) {
        let mut verts:Vec<Vec3> = Vec::new();
        let mut norms:Vec<Vec3> = Vec::new();
        let mut colors:Vec<[f32; 4]> = Vec::new();
        let mut last_qrz:Option<Qrz> = None;
        let mut skip_sw = false;
        let mut west_skirt_verts: Vec<Vec3> = Vec::new();
        let mut west_skirt_norms: Vec<Vec3> = Vec::new();
        let mut west_skirt_colors: Vec<[f32; 4]> = Vec::new();

        let map = self.0.clone();
        map.clone().into_iter().for_each(|tile| {
            let it_qrz = tile.0;
            let (it_vrt, it_col) = self.vertices_and_colors_with_slopes(it_qrz, apply_slopes);

            if let Some(last_qrz) = last_qrz {
                // if new column
                if last_qrz.q*2+last_qrz.r != it_qrz.q*2+it_qrz.r {
                    // add skirts
                    verts.append(&mut west_skirt_verts);
                    norms.append(&mut west_skirt_norms);
                    colors.append(&mut west_skirt_colors);
                }
            }

            let sw_result = self.find(it_qrz + Qrz{q:0,r:0,z:30} + qrz::DIRECTIONS[1], -60);
            let sw_data = sw_result.map(|(qrz, _)| self.vertices_and_colors_with_slopes(qrz, apply_slopes));

            if skip_sw {
                let (last_vrt, last_col) = self.vertices_and_colors_with_slopes(last_qrz.unwrap(), apply_slopes);
                let last_vrt_underover = Vec3::new(last_vrt[3].x, it_vrt[0].y, last_vrt[3].z);
                verts.extend([ last_vrt_underover, last_vrt_underover, it_vrt[0], it_vrt[0] ]);
                // For transition vertices, use simple up normal
                norms.extend([ Vec3::new(0., 1., 0.); 4 ]);
                colors.extend([ last_col[3], last_col[3], it_col[0], it_col[0] ]);
                skip_sw = false;
            }

            // Calculate smooth normals for the hex vertices
            let norm_0 = self.calculate_vertex_normal(it_qrz, 0, &it_vrt, apply_slopes);
            let norm_5 = self.calculate_vertex_normal(it_qrz, 5, &it_vrt, apply_slopes);
            let norm_4 = self.calculate_vertex_normal(it_qrz, 4, &it_vrt, apply_slopes);
            let norm_3 = self.calculate_vertex_normal(it_qrz, 3, &it_vrt, apply_slopes);
            let center_normal = Vec3::new(0., 1., 0.); // Center can stay up

            // Use height-based color for center vertex
            let it_center_color = self.height_color_tint(it_qrz.z);

            verts.extend([ it_vrt[0], it_vrt[5], it_vrt[6], it_vrt[4], it_vrt[3] ]);
            norms.extend([ norm_0, norm_5, center_normal, norm_4, norm_3 ]);
            colors.extend([ it_col[0], it_col[5], it_center_color, it_col[4], it_col[3] ]);

            if let Some((sw_vrt, sw_col)) = sw_data {
                // Calculate normals for southwest neighbor
                let sw_qrz = sw_result.unwrap().0;
                let sw_norm_0 = self.calculate_vertex_normal(sw_qrz, 0, &sw_vrt, apply_slopes);
                let sw_norm_1 = self.calculate_vertex_normal(sw_qrz, 1, &sw_vrt, apply_slopes);
                let sw_norm_2 = self.calculate_vertex_normal(sw_qrz, 2, &sw_vrt, apply_slopes);
                let sw_norm_3 = self.calculate_vertex_normal(sw_qrz, 3, &sw_vrt, apply_slopes);
                let sw_center = Vec3::new(0., 1., 0.);
                let sw_center_color = self.height_color_tint(sw_qrz.z);

                verts.extend([ sw_vrt[0], sw_vrt[1], sw_vrt[6], sw_vrt[2], sw_vrt[3]]);
                norms.extend([ sw_norm_0, sw_norm_1, sw_center, sw_norm_2, sw_norm_3 ]);
                colors.extend([ sw_col[0], sw_col[1], sw_center_color, sw_col[2], sw_col[3] ]);
            } else {
                verts.extend([ it_vrt[3] ]);
                norms.extend([ norm_3 ]);
                colors.extend([ it_col[3] ]);
                skip_sw = true;
            }

            let we_result = self.find(it_qrz + Qrz{q:0,r:0,z:30} + qrz::DIRECTIONS[0], -60);
            let we_qrz = we_result.unwrap_or((it_qrz + qrz::DIRECTIONS[0], EntityType::Decorator(default()))).0;
            // Only use sloped vertices if the tile actually exists in the map
            let (mut we_vrt, we_col) = if we_result.is_some() {
                self.vertices_and_colors_with_slopes(we_qrz, apply_slopes)
            } else {
                // For fake west neighbor, use height-based color
                let fake_we_color = self.height_color_tint(we_qrz.z);
                (self.0.vertices(we_qrz), vec![fake_we_color; 6])
            };
            
            // If west neighbor is fake, match its East edge vertices to current tile's West edge
            if we_result.is_none() {
                we_vrt[1].y = it_vrt[5].y;  // NE of west neighbor = NW of current tile  
                we_vrt[2].y = it_vrt[4].y;  // SE of west neighbor = SW of current tile
            }
            
            // Calculate normals for west neighbor (if it exists)
            let we_norm_1 = if we_result.is_some() {
                self.calculate_vertex_normal(we_qrz, 1, &we_vrt, apply_slopes)
            } else {
                Vec3::new(0., 1., 0.)
            };
            let we_norm_2 = if we_result.is_some() {
                self.calculate_vertex_normal(we_qrz, 2, &we_vrt, apply_slopes)
            } else {
                Vec3::new(0., 1., 0.)
            };

            if let Some(last_qrz) = last_qrz {
                let (last_vrt, last_col) = self.vertices_and_colors_with_slopes(last_qrz, apply_slopes);
                let last_vrt_underover = Vec3::new(it_vrt[5].x, last_vrt[4].y, it_vrt[5].z);
                west_skirt_verts.extend([ last_vrt_underover, last_vrt_underover ]);
                west_skirt_norms.extend([ Vec3::new(0., 1., 0.); 2 ]);
                west_skirt_colors.extend([ last_col[4], last_col[4] ]);
            }
            west_skirt_verts.extend([ it_vrt[5], we_vrt[1], it_vrt[4], we_vrt[2], it_vrt[4], it_vrt[4] ]);
            west_skirt_norms.extend([ norm_5, we_norm_1, norm_4, we_norm_2, norm_4, norm_4 ]);
            west_skirt_colors.extend([ it_col[5], we_col[1], it_col[4], we_col[2], it_col[4], it_col[4] ]);
            
            last_qrz = Some(it_qrz);
        });

        // Compute proper AABB from ALL vertices (not just column boundaries)
        // This ensures frustum culling works correctly with varied terrain heights
        let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
        let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
        for vert in &verts {
            min = Vec3::min(min, *vert);
            max = Vec3::max(max, *vert);
        }

        let len = verts.clone().len() as u32;
        println!("Terrain mesh: {} tiles, {} vertices, AABB: {:?} to {:?}",
                 self.0.len(), len, min, max);
        (
            Mesh::new(PrimitiveTopology::TriangleStrip, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, verts)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, (0..len).map(|_| [0., 0.]).collect::<Vec<[f32; 2]>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, norms)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
                .with_inserted_indices(Indices::U32((0..len).collect())),
            Aabb::from_min_max(min, max),
        )
    }

    pub fn find(&self, qrz: Qrz, dist: i8) -> Option<(Qrz, EntityType)> { self.0.find(qrz, dist) }
    pub fn get(&self, qrz: Qrz) -> Option<&EntityType> { self.0.get(qrz) }
    pub fn insert(&mut self, qrz: Qrz, obj: EntityType) { self.0.insert(qrz, obj); }
    pub fn remove(&mut self, qrz: Qrz) -> Option<EntityType> { self.0.remove(qrz) }
    pub fn len(&self) -> usize { self.0.len() }
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

#[cfg(test)]
mod tests {
    use super::*;
    use qrz::Qrz;

    #[test]
    fn test_smooth_vertex_normals_on_slope() {
        // Create a simple 2-tile map with elevation difference
        let mut qrz_map = qrz::Map::new(1.0, 0.8);
        let lower_tile = Qrz { q: 0, r: 0, z: 0 };
        let upper_tile = Qrz { q: 1, r: 0, z: 2 }; // 2 levels higher (cliff)

        qrz_map.insert(lower_tile, EntityType::Decorator(default()));
        qrz_map.insert(upper_tile, EntityType::Decorator(default()));

        let map = Map::new(qrz_map);
        let (mesh, _aabb) = map.regenerate_mesh(true);

        // Get normals from the mesh
        let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            .expect("Mesh should have normals")
            .as_float3()
            .expect("Normals should be Vec3");

        // At least some normals should NOT be straight up [0, 1, 0]
        // On a slope, normals should tilt to reflect the terrain angle
        let all_straight_up = normals.iter().all(|&n| {
            (n[0] - 0.0).abs() < 0.001 && (n[1] - 1.0).abs() < 0.001 && (n[2] - 0.0).abs() < 0.001
        });

        assert!(
            !all_straight_up,
            "Expected some normals to be tilted on sloped terrain, but all were [0, 1, 0]"
        );

        // All normals should be normalized (length ~= 1.0)
        for normal in normals {
            let length = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2)).sqrt();
            assert!(
                (length - 1.0).abs() < 0.01,
                "Normal {:?} should be normalized, but has length {}",
                normal,
                length
            );
        }
    }

    #[test]
    fn test_normals_consider_neighboring_hexes() {
        // Create two adjacent flat hexes at same elevation
        // If normals only consider the current hex, they'll be tilted toward/away from neighbors
        // If normals consider neighbors too, they should point straight up (smooth flat plane)
        let mut qrz_map = qrz::Map::new(1.0, 0.8);
        let hex1 = Qrz { q: 0, r: 0, z: 0 };
        let hex2 = Qrz { q: 1, r: 0, z: 0 }; // Adjacent hex at same elevation

        qrz_map.insert(hex1, EntityType::Decorator(default()));
        qrz_map.insert(hex2, EntityType::Decorator(default()));

        let map = Map::new(qrz_map);

        // Get vertices for hex1 to understand its structure
        let (hex1_verts, _) = map.vertices_and_colors_with_slopes(hex1, true);

        // Calculate normal for the vertex that's shared between hex1 and hex2
        // Vertex 1 (NE) of hex1 points toward hex2 (which is to the East, direction index 3)
        // Actually, hex2 is at direction index 3 (East), so vertices 1 and 2 are shared
        let shared_vertex_normal = map.calculate_vertex_normal(hex1, 1, &hex1_verts, true);

        // On a flat plane with neighbors, the normal should point straight up
        // If we only considered the current hex's triangles, it would be tilted
        // This tests that we're considering the neighboring hex's triangles too

        // The Y component should dominate (close to 1.0)
        assert!(
            shared_vertex_normal.y > 0.95,
            "Expected shared vertex normal to point mostly upward (Y > 0.95) on flat adjacent hexes, \
             but got normal: {:?} with Y = {}. This suggests normals aren't considering neighboring hexes.",
            shared_vertex_normal,
            shared_vertex_normal.y
        );

        // X and Z should be very small
        assert!(
            shared_vertex_normal.x.abs() < 0.3,
            "Expected X component of normal to be small on flat terrain, but got {}",
            shared_vertex_normal.x
        );
        assert!(
            shared_vertex_normal.z.abs() < 0.3,
            "Expected Z component of normal to be small on flat terrain, but got {}",
            shared_vertex_normal.z
        );
    }

    #[test]
    fn test_height_based_color_gradients() {
        // Create a map with tiles at different elevations
        let mut qrz_map = qrz::Map::new(1.0, 0.8);
        let low_tile = Qrz { q: 0, r: 0, z: 0 };   // Sea level
        let mid_tile = Qrz { q: 1, r: 0, z: 5 };   // Mid elevation
        let high_tile = Qrz { q: 2, r: 0, z: 10 }; // High elevation

        qrz_map.insert(low_tile, EntityType::Decorator(default()));
        qrz_map.insert(mid_tile, EntityType::Decorator(default()));
        qrz_map.insert(high_tile, EntityType::Decorator(default()));

        let map = Map::new(qrz_map);
        let (mesh, _aabb) = map.regenerate_mesh(true);

        // Get colors from the mesh
        let color_attr = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            .expect("Mesh should have colors");

        let colors = match color_attr {
            bevy_mesh::VertexAttributeValues::Float32x4(colors) => colors,
            _ => panic!("Expected Float32x4 color attribute"),
        };

        // Find colors for each tile's center vertex (we know the mesh structure from regenerate_mesh)
        // The colors should vary based on elevation

        // At least some colors should differ based on elevation
        // We expect lower tiles to be darker and higher tiles to be lighter (or have different hues)
        let unique_colors: std::collections::HashSet<String> = colors.iter()
            .filter(|c| {
                // Filter out cliff colors (gray-brown) to focus on grass colors
                !((c[0] - 0.35).abs() < 0.01 && (c[1] - 0.32).abs() < 0.01)
            })
            .map(|c| format!("{:.3},{:.3},{:.3}", c[0], c[1], c[2]))
            .collect();

        assert!(
            unique_colors.len() > 1,
            "Expected multiple different colors based on elevation, but found only {} unique grass colors. \
             Colors should vary with height.",
            unique_colors.len()
        );

        // All colors should be valid (components between 0 and 1)
        for color in colors {
            assert!(color[0] >= 0.0 && color[0] <= 1.0, "Red component out of range: {}", color[0]);
            assert!(color[1] >= 0.0 && color[1] <= 1.0, "Green component out of range: {}", color[1]);
            assert!(color[2] >= 0.0 && color[2] <= 1.0, "Blue component out of range: {}", color[2]);
            assert!(color[3] >= 0.0 && color[3] <= 1.0, "Alpha component out of range: {}", color[3]);
        }
    }

    #[test]
    fn test_generate_chunk_mesh() {
        use crate::common::chunk::{ChunkId, chunk_to_tile, CHUNK_SIZE};

        // Create a map with tiles in multiple chunks
        let mut qrz_map = qrz::Map::new(1.0, 0.8);

        // Chunk (0,0) - add 16 tiles
        for offset_q in 0..16 {
            for offset_r in 0..16 {
                let tile = chunk_to_tile(ChunkId(0, 0), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        // Chunk (1,1) - add 16 tiles
        for offset_q in 0..16 {
            for offset_r in 0..16 {
                let tile = chunk_to_tile(ChunkId(1, 1), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        let map = Map::new(qrz_map);

        // Generate mesh for chunk (0,0) only
        let (mesh, aabb) = map.generate_chunk_mesh(ChunkId(0, 0), true);

        // Verify mesh properties
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        // Each hex has 7 vertices (1 center + 6 outer)
        // 16x16 = 256 tiles per chunk
        assert_eq!(positions.len(), 256 * 7, "Expected 256 tiles * 7 vertices per tile");

        // Verify mesh has indices for TriangleList
        let indices = match mesh.indices() {
            Some(bevy_mesh::Indices::U32(idx)) => idx,
            _ => panic!("Expected U32 indices"),
        };

        // Each hex has 6 triangles (18 indices)
        assert_eq!(indices.len(), 256 * 6 * 3, "Expected 256 tiles * 6 triangles * 3 indices");

        // Verify AABB is reasonable (not empty)
        assert!(aabb.min().x < aabb.max().x, "AABB should have width");
        assert!(aabb.min().y < aabb.max().y, "AABB should have height");
        assert!(aabb.min().z < aabb.max().z, "AABB should have depth");
    }

    #[test]
    fn test_generate_chunk_mesh_filters_to_chunk() {
        use crate::common::chunk::{ChunkId, chunk_to_tile};

        // Create a map with tiles in two different chunks
        let mut qrz_map = qrz::Map::new(1.0, 0.8);

        // Chunk (0,0) - 4 tiles
        for offset_q in 0..2 {
            for offset_r in 0..2 {
                let tile = chunk_to_tile(ChunkId(0, 0), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        // Chunk (1,1) - 9 tiles
        for offset_q in 0..3 {
            for offset_r in 0..3 {
                let tile = chunk_to_tile(ChunkId(1, 1), offset_q as u8, offset_r as u8);
                qrz_map.insert(tile, EntityType::Decorator(default()));
            }
        }

        let map = Map::new(qrz_map);

        // Generate mesh for chunk (0,0) - should only include 4 tiles
        let (mesh_00, _) = map.generate_chunk_mesh(ChunkId(0, 0), true);
        let positions_00 = mesh_00.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        assert_eq!(positions_00.len(), 4 * 7, "Chunk (0,0) should have 4 tiles * 7 vertices");

        // Generate mesh for chunk (1,1) - should only include 9 tiles
        let (mesh_11, _) = map.generate_chunk_mesh(ChunkId(1, 1), true);
        let positions_11 = mesh_11.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh should have positions")
            .as_float3()
            .expect("Positions should be Vec3");

        assert_eq!(positions_11.len(), 9 * 7, "Chunk (1,1) should have 9 tiles * 7 vertices");
    }

    #[test]
    fn test_ambient_occlusion_darkens_enclosed_vertices() {
        // Test: Compare a vertex with one higher neighbor vs two higher neighbors
        // Both should get cliff colors, but the one with two neighbors should be darker (more AO)

        // Setup 1: Tile with one higher neighbor
        let mut map1 = qrz::Map::new(1.0, 0.8);
        let tile1 = Qrz { q: 0, r: 0, z: 0 };
        let neighbor1 = tile1 + qrz::DIRECTIONS[0] + Qrz { q: 0, r: 0, z: 3 }; // Higher to the west

        map1.insert(tile1, EntityType::Decorator(default()));
        map1.insert(neighbor1, EntityType::Decorator(default()));
        let map_one_neighbor = Map::new(map1);
        let (_, colors_one) = map_one_neighbor.vertices_and_colors_with_slopes(tile1, true);

        // Setup 2: Tile with two higher neighbors (same tile, but add another higher neighbor)
        let mut map2 = qrz::Map::new(1.0, 0.8);
        let tile2 = Qrz { q: 0, r: 0, z: 0 };
        let neighbor2a = tile2 + qrz::DIRECTIONS[0] + Qrz { q: 0, r: 0, z: 3 }; // Higher to the west
        let neighbor2b = tile2 + qrz::DIRECTIONS[5] + Qrz { q: 0, r: 0, z: 3 }; // Higher to the northwest

        map2.insert(tile2, EntityType::Decorator(default()));
        map2.insert(neighbor2a, EntityType::Decorator(default()));
        map2.insert(neighbor2b, EntityType::Decorator(default()));
        let map_two_neighbors = Map::new(map2);
        let (_, colors_two) = map_two_neighbors.vertices_and_colors_with_slopes(tile2, true);

        // Vertex 5 is shared by both higher neighbors (at the corner)
        // It should be darker in the two-neighbor case due to cumulative AO
        let brightness_one = colors_one[5][0] + colors_one[5][1] + colors_one[5][2];
        let brightness_two = colors_two[5][0] + colors_two[5][1] + colors_two[5][2];

        assert!(
            brightness_two < brightness_one,
            "Expected vertex with two higher neighbors to be darker due to cumulative AO, \
             but two-neighbor brightness ({}) >= one-neighbor brightness ({})",
            brightness_two,
            brightness_one
        );

        // The darkening should be moderate (not completely black)
        assert!(
            brightness_two > brightness_one * 0.3,
            "AO darkening should be subtle, but two-neighbor vertex is too dark: {} vs one-neighbor {}",
            brightness_two,
            brightness_one
        );
    }
}
