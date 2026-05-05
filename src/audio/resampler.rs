use crate::audio::commands::InterpolationType;
use crate::sequencer::sample::LoopType;

pub fn resample(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    interpolation: InterpolationType,
    loop_type: LoopType,
    direction: f64,
) -> f32 {
    match interpolation {
        InterpolationType::Nearest => resample_nearest(sample_data, position, loop_start, loop_end, loop_type, direction),
        InterpolationType::Linear => resample_linear(sample_data, position, loop_start, loop_end, loop_type, direction),
        InterpolationType::Cubic => resample_cubic(sample_data, position, loop_start, loop_end, loop_type, direction),
    }
}

fn wrap_index_forward(idx: usize, loop_start: usize, loop_end: usize) -> usize {
    if loop_end > loop_start {
        if idx < loop_start {
            let loop_len = loop_end - loop_start;
            let offset = (loop_start - idx - 1) % loop_len;
            loop_end - 1 - offset
        } else if idx >= loop_end {
            let loop_len = loop_end - loop_start;
            loop_start + (idx - loop_start) % loop_len
        } else {
            idx
        }
    } else {
        idx
    }
}

fn wrap_index_pingpong(idx: usize, loop_start: usize, loop_end: usize, _direction: f64) -> usize {
    if loop_end <= loop_start {
        return idx;
    }
    let loop_len = loop_end - loop_start;
    if idx < loop_start {
        loop_start + (loop_start - 1 - idx) % loop_len
    } else if idx >= loop_end {
        loop_start + (idx - loop_start) % loop_len
    } else {
        idx
    }
}

fn get_sample_looped(
    sample_data: &[f32],
    index: usize,
    loop_start: usize,
    loop_end: usize,
    loop_type: LoopType,
    direction: f64,
) -> f32 {
    let wrapped = match loop_type {
        LoopType::Forward => wrap_index_forward(index, loop_start, loop_end),
        LoopType::PingPong => wrap_index_pingpong(index, loop_start, loop_end, direction),
        _ => index,
    };
    if wrapped < sample_data.len() {
        sample_data[wrapped]
    } else {
        0.0
    }
}

fn resample_nearest(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    loop_type: LoopType,
    direction: f64,
) -> f32 {
    let index = position as usize;
    if index >= sample_data.len() {
        let idx = match loop_type {
            LoopType::Forward if loop_end > loop_start => wrap_index_forward(index, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start => wrap_index_pingpong(index, loop_start, loop_end, direction),
            _ => return 0.0,
        };
        if idx < sample_data.len() { sample_data[idx] } else { 0.0 }
    } else {
        let idx = match loop_type {
            LoopType::Forward if loop_end > loop_start && index >= loop_end => wrap_index_forward(index, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start && index >= loop_end => wrap_index_pingpong(index, loop_start, loop_end, direction),
            _ => index,
        };
        if idx < sample_data.len() { sample_data[idx] } else { 0.0 }
    }
}

fn resample_linear(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    loop_type: LoopType,
    direction: f64,
) -> f32 {
    let index0 = position as usize;
    let frac = position - index0 as f64;

    if index0 >= sample_data.len() {
        let i0 = match loop_type {
            LoopType::Forward if loop_end > loop_start => wrap_index_forward(index0, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start => wrap_index_pingpong(index0, loop_start, loop_end, direction),
            _ => return 0.0,
        };
        let s0 = if i0 < sample_data.len() { sample_data[i0] as f64 } else { 0.0 };
        let i1 = i0 + 1;
        let s1 = get_sample_looped(sample_data, i1, loop_start, loop_end, loop_type, direction) as f64;
        return (s0 + (s1 - s0) * frac) as f32;
    }

    let s0 = get_sample_looped(sample_data, index0, loop_start, loop_end, loop_type, direction) as f64;

    let index1 = index0 + 1;

    let s1 = get_sample_looped(sample_data, index1, loop_start, loop_end, loop_type, direction) as f64;

    (s0 + (s1 - s0) * frac) as f32
}

fn resample_cubic(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    loop_type: LoopType,
    direction: f64,
) -> f32 {
    let index1 = position.floor() as usize;
    let frac = position - index1 as f64;
    let len = sample_data.len();

    if index1 >= len && !(loop_end > loop_start && matches!(loop_type, LoopType::Forward | LoopType::PingPong)) {
        return 0.0;
    }

    let i0 = index1.saturating_sub(1);
    let i2 = index1.wrapping_add(1);
    let i3 = index1.wrapping_add(2);

    let y0 = get_sample_looped(sample_data, i0, loop_start, loop_end, loop_type, direction) as f64;
    let y1 = get_sample_looped(sample_data, index1, loop_start, loop_end, loop_type, direction) as f64;
    let y2 = get_sample_looped(sample_data, i2, loop_start, loop_end, loop_type, direction) as f64;
    let y3 = get_sample_looped(sample_data, i3, loop_start, loop_end, loop_type, direction) as f64;

    let a = (-y0 + 3.0 * y1 - 3.0 * y2 + y3) / 2.0;
    let b = y0 - 5.0 * y1 / 2.0 + 2.0 * y2 - y3 / 2.0;
    let c = (-y0 + y2) / 2.0;
    let d = y1;

    (a * frac * frac * frac + b * frac * frac + c * frac + d) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_at_integer_position() {
        let data = [0.0, 0.5, 1.0, 0.5, 0.0];
        assert!((resample_nearest(&data, 0.0, 0, data.len(), LoopType::None, 1.0) - 0.0).abs() < 0.001);
        assert!((resample_nearest(&data, 1.0, 0, data.len(), LoopType::None, 1.0) - 0.5).abs() < 0.001);
        assert!((resample_nearest(&data, 2.0, 0, data.len(), LoopType::None, 1.0) - 1.0).abs() < 0.001);
    }

    #[test]
    fn nearest_beyond_end() {
        let data = [0.0, 0.5, 1.0];
        assert!((resample_nearest(&data, 5.0, 0, data.len(), LoopType::None, 1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn linear_midpoint() {
        let data = [0.0, 1.0];
        let result = resample_linear(&data, 0.5, 0, data.len(), LoopType::None, 1.0);
        assert!((result - 0.5).abs() < 0.001);
    }

    #[test]
    fn linear_at_sample() {
        let data = [0.2, 0.8, 0.4];
        assert!((resample_linear(&data, 0.0, 0, data.len(), LoopType::None, 1.0) - 0.2).abs() < 0.001);
        assert!((resample_linear(&data, 1.0, 0, data.len(), LoopType::None, 1.0) - 0.8).abs() < 0.001);
    }

    #[test]
    fn linear_loop_wrap() {
        let data = [0.0, 0.5, 1.0, 0.8];
        let result = resample_linear(&data, 2.5, 1, 3, LoopType::Forward, 1.0);
        let s0 = data[2];
        let s1 = data[1];
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001);
    }

    #[test]
    fn cubic_at_sample_point() {
        let data = [0.0, 0.5, 1.0, 0.5, 0.0];
        let result = resample_cubic(&data, 1.0, 0, data.len(), LoopType::None, 1.0);
        assert!((result - 0.5).abs() < 0.001);
    }

    #[test]
    fn cubic_beyond_end() {
        let data = [0.0, 0.5, 1.0];
        assert!((resample_cubic(&data, 5.0, 0, data.len(), LoopType::None, 1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn linear_loop_wrap_index0() {
        let data = [0.0, 0.1, 0.5, 1.0, 0.5, 0.0, -0.8, -0.9];
        let result = resample_linear(&data, 6.5, 2, 6, LoopType::Forward, 1.0);
        let s0 = data[4];
        let s1 = data[3];
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001,
            "index0=6 should wrap to 4, index1=7 wraps to 3");
    }

    #[test]
    fn nearest_loop_wrap() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let result = resample_nearest(&data, 5.0, 2, 5, LoopType::Forward, 1.0);
        assert!((result - 3.0).abs() < 0.001, "index 5 should wrap to 2");
    }

    #[test]
    fn loop_wrap_does_not_affect_non_looped() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let result = resample_linear(&data, 2.0, 0, 0, LoopType::None, 1.0);
        assert!((result - 3.0).abs() < 0.001, "Non-looped sample at index 2 should read value 3.0");
    }

    #[test]
    fn resample_dispatches_correctly() {
        let data = [0.0, 0.5, 1.0, 0.5, 0.0];
        let r_nearest = resample(&data, 1.0, 0, 5, InterpolationType::Nearest, LoopType::None, 1.0);
        let r_linear = resample(&data, 1.0, 0, 5, InterpolationType::Linear, LoopType::None, 1.0);
        let r_cubic = resample(&data, 1.0, 0, 5, InterpolationType::Cubic, LoopType::None, 1.0);

        assert!((r_nearest - 0.5).abs() < 0.001);
        assert!((r_linear - 0.5).abs() < 0.001);
        assert!((r_cubic - 0.5).abs() < 0.001);
    }

    #[test]
    fn pingpong_forward_wraps_to_loop_end_minus_1() {
        let data = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
        let result = resample_linear(&data, 5.5, 2, 6, LoopType::PingPong, 1.0);
        assert!((result - 45.0).abs() < 0.001,
            "index1=6 wraps to loop_start=2 via cyclic wrap for PingPong forward, lerp(60,30)=45, got {result}");
    }

    #[test]
    fn pingpong_backward_wraps_below_loop_start() {
        let data = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
        let result = resample_linear(&data, 2.5, 2, 6, LoopType::PingPong, -1.0);
        assert!((result - 35.0).abs() < 0.001,
            "index1=1 should clamp to loop_start=2 for PingPong backward, got {result}");
    }
}
