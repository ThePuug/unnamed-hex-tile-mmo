//! Every texture, by name. Adding one: a module with
//! `pub fn build(&Params) -> Canvas`, an entry in `TEXTURES`, a row in the
//! README.

mod cliff_stone;
mod grass_plain;

use crate::canvas::Canvas;

pub struct Params {
    /// Edge length in pixels.
    pub size: u32,
    pub seed: u64,
}

pub struct Texture {
    pub name: &'static str,
    /// What the tile must read as. The comparison critic judges against it.
    pub brief: &'static str,
    pub build: fn(&Params) -> Canvas,
}

pub const TEXTURES: &[Texture] = &[
    Texture {
        name: "grass-plain",
        brief: "Lowland turf for hex tile tops: the terrain ramp's plain green, \
                unevenly grown, with a few patches worn through to bare earth.",
        build: grass_plain::build,
    },
    Texture {
        name: "cliff-stone",
        brief: "Fractured stone for cliff faces and skirts: the terrain shader's cliff \
                grey, a face broken into fragments with varying numbers of cracked \
                edges, slabs wider than tall with some shattered into shards, a \
                little moss in the deeper cracks.",
        build: cliff_stone::build,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_texture_is_deterministic_and_tiles() {
        let p = Params { size: 128, seed: 7 };
        for t in TEXTURES {
            let a = (t.build)(&p);
            let b = (t.build)(&p);
            assert_eq!(a.to_image().as_raw(), b.to_image().as_raw(), "{} differs between runs", t.name);
            let seam = a.seam_ratio();
            assert!(seam <= crate::SEAM_LIMIT, "{} seam ratio {seam:.2} past {}", t.name, crate::SEAM_LIMIT);
        }
    }
}
