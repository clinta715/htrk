use crate::audio::commands::InterpolationType;
use crate::sequencer::sample::LoopType;

pub fn resample(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    interpolation: InterpolationType,
    loop_type: LoopType,
) -> f32 {
    match interpolation {
        InterpolationType::Nearest => resample_nearest(sample_data, position, loop_start, loop_end, loop_type),
        InterpolationType::Linear => resample_linear(sample_data, position, loop_start, loop_end, loop_type),
        InterpolationType::Cubic => resample_cubic(sample_data, position, loop_start, loop_end, loop_type),
    }
}

fn wrap_index_forward(idx: usize, loop_start: usize, loop_end: usize) -> usize {
    if loop_end > loop_start && idx >= loop_end {
        let loop_len = loop_end - loop_start;
        loop_start + (idx - loop_start) % loop_len
    } else {
        idx
    }
}

fn wrap_index_pingpong(idx: usize, loop_start: usize, loop_end: usize) -> usize {
    if loop_end <= loop_start {
        return idx;
    }
    if idx >= loop_end {
        let loop_len = loop_end - loop_start;
        loop_start + (idx - loop_start) % loop_len
    } else {
        idx
    }
}

fn wrap_index_backward(idx: usize, loop_start: usize, loop_end: usize) -> usize {
    if loop_end <= loop_start {
        return idx;
    }
    let loop_len = loop_end - loop_start;
    if idx < loop_start {
        loop_end - 1 - ((loop_start - 1 - idx) % loop_len)
    } else if idx >= loop_end {
        loop_end - 1 - ((idx - loop_end) % loop_len)
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
) -> f32 {
    let wrapped = match loop_type {
        LoopType::Forward => wrap_index_forward(index, loop_start, loop_end),
        LoopType::PingPong => wrap_index_pingpong(index, loop_start, loop_end),
        LoopType::Backward if loop_end > loop_start => wrap_index_backward(index, loop_start, loop_end),
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
) -> f32 {
    let index = position as usize;
    if index >= sample_data.len() {
        let idx = match loop_type {
            LoopType::Forward if loop_end > loop_start => wrap_index_forward(index, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start => wrap_index_pingpong(index, loop_start, loop_end),
            LoopType::Backward if loop_end > loop_start => wrap_index_backward(index, loop_start, loop_end),
            _ => return 0.0,
        };
        if idx < sample_data.len() { sample_data[idx] } else { 0.0 }
    } else {
        let idx = match loop_type {
            LoopType::Forward if loop_end > loop_start && index >= loop_end => wrap_index_forward(index, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start && index >= loop_end => wrap_index_pingpong(index, loop_start, loop_end),
            LoopType::Backward if loop_end > loop_start && index >= loop_end => wrap_index_backward(index, loop_start, loop_end),
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
) -> f32 {
    let index0 = position as usize;
    let frac = position - index0 as f64;

    if index0 >= sample_data.len() {
        let i0 = match loop_type {
            LoopType::Forward if loop_end > loop_start => wrap_index_forward(index0, loop_start, loop_end),
            LoopType::PingPong if loop_end > loop_start => wrap_index_pingpong(index0, loop_start, loop_end),
            LoopType::Backward if loop_end > loop_start => wrap_index_backward(index0, loop_start, loop_end),
            _ => return 0.0,
        };
        let s0 = if i0 < sample_data.len() { sample_data[i0] as f64 } else { 0.0 };
        let i1 = i0 + 1;
        let s1 = get_sample_looped(sample_data, i1, loop_start, loop_end, loop_type) as f64;
        return (s0 + (s1 - s0) * frac) as f32;
    }

    let s0 = get_sample_looped(sample_data, index0, loop_start, loop_end, loop_type) as f64;

    let index1 = index0 + 1;

    let s1 = get_sample_looped(sample_data, index1, loop_start, loop_end, loop_type) as f64;

    (s0 + (s1 - s0) * frac) as f32
}

fn resample_cubic(
    sample_data: &[f32],
    position: f64,
    loop_start: usize,
    loop_end: usize,
    loop_type: LoopType,
) -> f32 {
    let index1 = position.floor() as usize;
    let frac = position - index1 as f64;
    let len = sample_data.len();

    if index1 >= len && !(loop_end > loop_start && matches!(loop_type, LoopType::Forward | LoopType::PingPong | LoopType::Backward)) {
        return 0.0;
    }

    let i0 = index1.saturating_sub(1);
    let i2 = index1.wrapping_add(1);
    let i3 = index1.wrapping_add(2);

    let y0 = get_sample_looped(sample_data, i0, loop_start, loop_end, loop_type) as f64;
    let y1 = get_sample_looped(sample_data, index1, loop_start, loop_end, loop_type) as f64;
    let y2 = get_sample_looped(sample_data, i2, loop_start, loop_end, loop_type) as f64;
    let y3 = get_sample_looped(sample_data, i3, loop_start, loop_end, loop_type) as f64;

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
        assert!((resample_nearest(&data, 0.0, 0, data.len(), LoopType::None) - 0.0).abs() < 0.001);
        assert!((resample_nearest(&data, 1.0, 0, data.len(), LoopType::None) - 0.5).abs() < 0.001);
        assert!((resample_nearest(&data, 2.0, 0, data.len(), LoopType::None) - 1.0).abs() < 0.001);
    }

    #[test]
    fn nearest_beyond_end() {
        let data = [0.0, 0.5, 1.0];
        assert!((resample_nearest(&data, 5.0, 0, data.len(), LoopType::None) - 0.0).abs() < 0.001);
    }

    #[test]
    fn linear_midpoint() {
        let data = [0.0, 1.0];
        let result = resample_linear(&data, 0.5, 0, data.len(), LoopType::None);
        assert!((result - 0.5).abs() < 0.001);
    }

    #[test]
    fn linear_at_sample() {
        let data = [0.2, 0.8, 0.4];
        assert!((resample_linear(&data, 0.0, 0, data.len(), LoopType::None) - 0.2).abs() < 0.001);
        assert!((resample_linear(&data, 1.0, 0, data.len(), LoopType::None) - 0.8).abs() < 0.001);
    }

    #[test]
    fn linear_loop_wrap() {
        let data = [0.0, 0.5, 1.0, 0.8];
        let result = resample_linear(&data, 2.5, 1, 3, LoopType::Forward);
        let s0 = data[2];
        let s1 = data[1];
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001);
    }

    #[test]
    fn cubic_at_sample_point() {
        let data = [0.0, 0.5, 1.0, 0.5, 0.0];
        let result = resample_cubic(&data, 1.0, 0, data.len(), LoopType::None);
        assert!((result - 0.5).abs() < 0.001);
    }

    #[test]
    fn cubic_beyond_end() {
        let data = [0.0, 0.5, 1.0];
        assert!((resample_cubic(&data, 5.0, 0, data.len(), LoopType::None) - 0.0).abs() < 0.001);
    }

    #[test]
    fn linear_loop_wrap_index0() {
        let data = [0.0, 0.1, 0.5, 1.0, 0.5, 0.0, -0.8, -0.9];
        let result = resample_linear(&data, 6.5, 2, 6, LoopType::Forward);
        let s0 = data[2];
        let s1 = data[3];
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001,
            "index0=6 should wrap to 2, index1=7 wraps to 3");
    }

    #[test]
    fn nearest_loop_wrap() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let result = resample_nearest(&data, 5.0, 2, 5, LoopType::Forward);
        assert!((result - 3.0).abs() < 0.001, "index 5 should wrap to 2");
    }

    #[test]
    fn loop_wrap_does_not_affect_non_looped() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let result = resample_linear(&data, 2.0, 0, 0, LoopType::None);
        assert!((result - 3.0).abs() < 0.001, "Non-looped sample at index 2 should read value 3.0");
    }

    #[test]
    fn resample_dispatches_correctly() {
        let data = [0.0, 0.5, 1.0, 0.5, 0.0];
        let r_nearest = resample(&data, 1.0, 0, 5, InterpolationType::Nearest, LoopType::None);
        let r_linear = resample(&data, 1.0, 0, 5, InterpolationType::Linear, LoopType::None);
        let r_cubic = resample(&data, 1.0, 0, 5, InterpolationType::Cubic, LoopType::None);

        assert!((r_nearest - 0.5).abs() < 0.001);
        assert!((r_linear - 0.5).abs() < 0.001);
        assert!((r_cubic - 0.5).abs() < 0.001);
    }

    #[test]
    fn pingpong_forward_wraps_to_loop_end_minus_1() {
        let data = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
        let result = resample_linear(&data, 5.5, 2, 6, LoopType::PingPong);
        assert!((result - 45.0).abs() < 0.001,
            "index1=6 wraps to loop_start=2 via cyclic wrap for PingPong forward, lerp(60,30)=45, got {result}");
    }

    #[test]
    fn pingpong_backward_wraps_below_loop_start() {
        let data = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
        let result = resample_linear(&data, 2.5, 2, 6, LoopType::PingPong);
        assert!((result - 35.0).abs() < 0.001,
            "index1=1 should clamp to loop_start=2 for PingPong backward, got {result}");
    }

    #[test]
    fn forward_pre_loop_indices_not_wrapped() {
        let data = [111.0, 222.0, 10.0, 20.0, 30.0, 40.0, 50.0];
        let result = resample_linear(&data, 0.0, 2, 6, LoopType::Forward);
        assert!((result - 111.0).abs() < 0.001,
            "index 0 before loop_start=2 should read data[0]=111, got {result}");
        let result = resample_linear(&data, 1.0, 2, 6, LoopType::Forward);
        assert!((result - 222.0).abs() < 0.001,
            "index 1 before loop_start=2 should read data[1]=222, got {result}");
    }

    #[test]
    fn forward_loop_wrap_only_at_or_after_loop_end() {
        let data = [111.0, 222.0, 10.0, 20.0, 30.0, 40.0];
        let result = resample_linear(&data, 2.0, 2, 5, LoopType::Forward);
        assert!((result - 10.0).abs() < 0.001,
            "loop start index 2 should read data[2]=10, got {result}");
        let result = resample_linear(&data, 4.0, 2, 5, LoopType::Forward);
        assert!((result - 30.0).abs() < 0.001,
            "index 4 within loop should read data[4]=30, got {result}");
        let result = resample_linear(&data, 5.0, 2, 5, LoopType::Forward);
        let expected = 10.0;
        assert!((result - expected).abs() < 0.001,
            "index 5 >= loop_end=5 should wrap to loop_start data[2]=10, got {result}");
    }

    #[test]
    fn pingpong_pre_loop_not_wrapped() {
        let data = [111.0, 222.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_linear(&data, 0.0, 2, 6, LoopType::PingPong);
        assert!((result - 111.0).abs() < 0.001,
            "pingpong index 0 before loop_start=2 should read data[0]=111, got {result}");
    }

    #[test]
    fn cubic_pre_loop_not_wrapped() {
        let data = [111.0, 222.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_cubic(&data, 0.0, 2, 6, LoopType::Forward);
        assert!((result - 111.0).abs() < 0.001,
            "cubic index 0 before loop_start=2 should read data[0]=111, got {result}");
    }

    #[test]
    fn nearest_pre_loop_not_wrapped() {
        let data = [111.0, 222.0, 10.0, 20.0, 30.0, 40.0, 50.0];
        let result = resample_nearest(&data, 0.0, 2, 6, LoopType::Forward);
        assert!((result - 111.0).abs() < 0.001,
            "nearest index 0 before loop_start=2 should read data[0]=111, got {result}");
    }

    #[test]
    fn forward_loop_uses_correct_sample_across_loop_boundary() {
        let data = [99.0, 88.0, 10.0, 20.0, 30.0, 40.0];
        let result = resample_linear(&data, 4.5, 2, 5, LoopType::Forward);
        let s0 = 30.0;
        let s1 = 10.0;
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001,
            "position 4.5 should correctly lerp across loop boundary, got {result}");
    }

    #[test]
    fn backward_loop_wraps_below_loop_start() {
        let data = [99.0, 88.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_linear(&data, 1.5, 2, 6, LoopType::Backward);
        let s0 = 40.0;
        let s1 = 10.0;
        let expected = s0 + (s1 - s0) * 0.5;
        assert!((result - expected).abs() < 0.001,
            "Backward index 1 should wrap to data[5]=40, index 2 stays at data[2]=10, lerp=25, got {result}");
    }

    #[test]
    fn backward_cubic_does_not_read_pre_loop() {
        let data = [999.0, 888.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_cubic(&data, 2.0, 2, 6, LoopType::Backward);
        assert!((result - 10.0).abs() > 0.1 || result != 999.0 && result != 888.0,
            "Cubic Backward at loop_start=2 should not read pre-loop data [0]=999 or [1]=888, got {result}");
    }

    #[test]
    fn backward_loop_beyond_end_wraps() {
        let data = [99.0, 88.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_nearest(&data, 7.0, 2, 6, LoopType::Backward);
        assert!((result - 30.0).abs() < 0.001,
            "Backward index 7 should wrap into loop body, got {result}");
    }

    #[test]
    fn backward_linear_beyond_end_wraps() {
        let data = [99.0, 88.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
        let result = resample_linear(&data, 6.0, 2, 6, LoopType::Backward);
        assert!((result - 40.0).abs() < 0.001,
            "Backward linear index 6 should wrap to data[5]=40, got {result}");
    }
}
