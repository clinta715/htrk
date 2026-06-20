#[derive(Clone, Copy, PartialEq)]
pub enum GeneratorShape {
    Sine,
    Square,
    Triangle,
    SawUp,
    SawDown,
    Pulse,
    Random,
}

pub fn generate_values(
    shape: GeneratorShape,
    length: u16,
    cycles: f32,
    depth: f32,
    offset: f32,
    duty: f32,
) -> Vec<(u16, f32)> {
    use std::f32::consts::TAU;

    let length_f = length as f32;
    let num_cycles = cycles.max(0.25);

    if length < 2 {
        return vec![(0, offset)];
    }

    let clamp_val = |v: f32| -> f32 { v.clamp(0.0, 1.0) };

    match shape {
        GeneratorShape::Sine => {
            let pts_per_cycle = 16usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize + 1;
            let mut points = Vec::with_capacity(total_pts);
            for i in 0..total_pts {
                let t = (i as f32 / (total_pts - 1) as f32) * length_f;
                let phase = num_cycles * TAU * t / length_f;
                let v = offset + (depth / 2.0) * phase.sin();
                points.push((t.round() as u16, clamp_val(v)));
            }
            points
        }
        GeneratorShape::Square => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset + depth / 2.0;
            let lo = offset - depth / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let mid = (cycle_start + cycle_end) / 2.0;
                points.push((cycle_start.round() as u16, clamp_val(hi)));
                points.push((mid.round() as u16, clamp_val(hi)));
                points.push((mid.round() as u16, clamp_val(lo)));
                points.push((cycle_end.round() as u16, clamp_val(lo)));
            }
            points.sort_by_key(|p| p.0);
            points.dedup_by_key(|p| p.0);
            points
        }
        GeneratorShape::Triangle => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset + depth / 2.0;
            let lo = offset - depth / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let mid = (cycle_start + cycle_end) / 2.0;
                points.push((cycle_start.round() as u16, clamp_val(lo)));
                points.push((mid.round() as u16, clamp_val(hi)));
                points.push((cycle_end.round() as u16, clamp_val(lo)));
            }
            points.sort_by_key(|p| p.0);
            points.dedup_by_key(|p| p.0);
            points
        }
        GeneratorShape::SawUp => {
            let pts_per_cycle = 2usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 2);
            let hi = offset + depth / 2.0;
            let lo = offset - depth / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                points.push((cycle_start.round() as u16, clamp_val(lo)));
                points.push((cycle_end.round() as u16, clamp_val(hi)));
            }
            points.sort_by_key(|p| p.0);
            points.dedup_by_key(|p| p.0);
            points
        }
        GeneratorShape::SawDown => {
            let pts_per_cycle = 2usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 2);
            let hi = offset + depth / 2.0;
            let lo = offset - depth / 2.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                points.push((cycle_start.round() as u16, clamp_val(hi)));
                points.push((cycle_end.round() as u16, clamp_val(lo)));
            }
            points.sort_by_key(|p| p.0);
            points.dedup_by_key(|p| p.0);
            points
        }
        GeneratorShape::Pulse => {
            let pts_per_cycle = 4usize;
            let total_pts = (pts_per_cycle as f32 * num_cycles).ceil() as usize;
            let mut points = Vec::with_capacity(total_pts + 1);
            let hi = offset + depth / 2.0;
            let lo = offset - depth / 2.0;
            let duty_f = duty / 100.0;
            for c in 0..(num_cycles.ceil() as usize) {
                let cycle_start = (c as f32 / num_cycles) * length_f;
                let cycle_end = ((c + 1) as f32 / num_cycles) * length_f;
                let transition = cycle_start + (cycle_end - cycle_start) * duty_f;
                points.push((cycle_start.round() as u16, clamp_val(hi)));
                points.push((transition.round() as u16, clamp_val(hi)));
                points.push((transition.round() as u16, clamp_val(lo)));
                points.push((cycle_end.round() as u16, clamp_val(lo)));
            }
            points.sort_by_key(|p| p.0);
            points.dedup_by_key(|p| p.0);
            points
        }
        GeneratorShape::Random => {
            let step = (length_f / (num_cycles * 8.0)).max(1.0).round() as u16;
            let num_pts = (length / step).max(2) as usize;
            let mut points = Vec::with_capacity(num_pts);
            let mut cur = offset;
            let half_depth = depth / 2.0;
            let mut seed: u32 = (length as u32) ^ (cycles as u32).wrapping_mul(12345) ^ ((depth * 100.0) as u32) * 6789;
            for i in 0..num_pts {
                let t = ((i as f32 / (num_pts - 1) as f32) * length_f).round() as u16;
                points.push((t, clamp_val(cur)));
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let delta = ((seed as f32) / (u32::MAX as f32) - 0.5) * half_depth * 0.5;
                cur = (cur + delta).clamp(offset - half_depth, offset + half_depth);
            }
            points
        }
    }
}
