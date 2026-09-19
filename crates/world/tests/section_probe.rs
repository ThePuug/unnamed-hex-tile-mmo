//! Section probe — the ground and the ranges' own relief along a line.
//! Run: SECTION="x0,y0,x1,y1,step" cargo test -p world --release --test section_probe -- --ignored --nocapture

use world::events::Composite;
use world::events::thrusting::Outlines;
use world::world_to_hex;

const SEED: u64 = 0x9E3779B97F4A7C15;

#[test]
#[ignore]
fn section() {
    let spec = std::env::var("SECTION").unwrap();
    let v: Vec<f64> = spec.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let (x0, y0, x1, y1, step) = (v[0], v[1], v[2], v[3], v[4]);
    let c = Composite::standard(SEED);
    let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let outlines = Outlines::in_box((x0 + x1) / 2.0, (y0 + y1) / 2.0, len / 2.0 + 10.0, SEED);
    let n = (len / step).ceil() as usize;
    println!("{:>8} {:>10} {:>10} {:>8} {:>8} {:>8}  plate", "s", "wx", "wy", "ground", "ranges", "water");
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let (wx, wy) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        let (q, r) = world_to_hex(wx, wy);
        let view = c.tile_at(q, r);
        let relief = outlines.relief(wx, wy);
        let plate = outlines.at(wx, wy).map(|(p, d)| {
            let ds: Vec<String> = p.edges.iter().zip(&d).map(|(e, d)| format!("{}{:.0}", if e.converge > 0.0 { "*" } else { "" }, d)).collect();
            format!("{:?} [{}]", p.id, ds.join(" "))
        }).unwrap_or_default();
        let water = c.water_at(q, r).map(|w| format!("{w}")).unwrap_or_default();
        println!("{:8.0} {:10.0} {:10.0} {:8.1} {:8.1} {:>8}  {plate}", t * len, wx, wy, view.elevation, relief, water);
    }
}
