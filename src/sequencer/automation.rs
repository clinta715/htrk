use serde::{Deserialize, Serialize};

const MAX_ROWS_FOR_TICK: u64 = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InterpolationMode {
    #[default]
    Hold,
    Linear,
    Smooth,
    Exponential,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutomationTarget {
    ChannelVolume,
    ChannelPanning,
    FilterCutoff,
    FilterResonance,
    SendLevel { bus: u8 },
    GlobalVolume,
    Tempo,
    Speed,
    SendReturnLevel { bus: u8 },
    SendBusParam { bus: u8, param: u8 },
    /// Automation on a CLAP plugin parameter. The `send_bus` is the
    /// send bus that owns the plugin (0..=3). The `param_id` is the
    /// plugin's stable `ClapId` (assigned by the plugin and stable
    /// across rescans). The `host_index` is the host-side index in
    /// `Module.send_bus_plugins[bus].param_info` (used by the UI to
    /// resolve the param name and range).
    #[serde(skip)] // not persisted; re-resolved on load from send_bus_plugins
    PluginParam { send_bus: u8, host_index: u32, param_id: u32 },

    /// Automation on a CLAP instrument plugin parameter. The
    /// `instrument` is the 1-based instrument index. The `param_id`
    /// is the plugin's stable `ClapId`; the `host_index` is the
    /// host-side index in the instrument's param_info cache.
    /// Re-resolved on load by walking `module.instruments[instrument].plugin`
    /// and re-enumerating the plugin's params.
    #[serde(skip)]
    InstrumentPluginParam { instrument: u8, host_index: u32, param_id: u32 },
}

impl AutomationTarget {
    pub fn is_global(self) -> bool {
        matches!(
            self,
            AutomationTarget::GlobalVolume
                | AutomationTarget::Tempo
                | AutomationTarget::Speed
                | AutomationTarget::SendReturnLevel { .. }
                | AutomationTarget::SendBusParam { .. }
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            AutomationTarget::ChannelVolume => "Volume",
            AutomationTarget::ChannelPanning => "Panning",
            AutomationTarget::FilterCutoff => "Flt Cut",
            AutomationTarget::FilterResonance => "Flt Res",
            AutomationTarget::SendLevel { bus } => match bus {
                0 => "Send A",
                1 => "Send B",
                2 => "Send C",
                3 => "Send D",
                _ => "Send ?",
            },
            AutomationTarget::GlobalVolume => "Glb Vol",
            AutomationTarget::Tempo => "Tempo",
            AutomationTarget::Speed => "Speed",
            AutomationTarget::SendReturnLevel { bus } => match bus {
                0 => "Ret A",
                1 => "Ret B",
                2 => "Ret C",
                3 => "Ret D",
                _ => "Ret ?",
            },
            AutomationTarget::SendBusParam { bus, param: _ } => match bus {
                0 => "FX A",
                1 => "FX B",
                2 => "FX C",
                3 => "FX D",
                _ => "FX ?",
            },
            // PluginParam labels are computed dynamically (from the
            // param_info) so this static label is just a placeholder.
            AutomationTarget::PluginParam { .. } => "Plugin",
            AutomationTarget::InstrumentPluginParam { instrument, .. } => match instrument {
                0 => "Inst",
                _ => "Inst",
            },
        }
    }

    pub fn is_multiplier(self) -> bool {
        matches!(
            self,
            AutomationTarget::ChannelVolume
                | AutomationTarget::FilterCutoff
                | AutomationTarget::SendLevel { .. }
                | AutomationTarget::GlobalVolume
                | AutomationTarget::Tempo
        )
    }

    pub fn all_per_channel() -> Vec<AutomationTarget> {
        let mut targets = vec![
            AutomationTarget::ChannelVolume,
            AutomationTarget::ChannelPanning,
            AutomationTarget::FilterCutoff,
            AutomationTarget::FilterResonance,
        ];
        for bus in 0..4u8 {
            targets.push(AutomationTarget::SendLevel { bus });
        }
        targets
    }

    pub fn all_global() -> Vec<AutomationTarget> {
        let mut targets = vec![
            AutomationTarget::GlobalVolume,
            AutomationTarget::Tempo,
            AutomationTarget::Speed,
        ];
        for bus in 0..4u8 {
            targets.push(AutomationTarget::SendReturnLevel { bus });
            for param in 0..4u8 {
                targets.push(AutomationTarget::SendBusParam { bus, param });
            }
        }
        targets
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub order: u16,
    pub row: u16,
    pub value: f32,
    pub interp_to_next: InterpolationMode,
}

impl AutomationPoint {
    pub fn song_tick(&self, speed: u8) -> u64 {
        (self.order as u64 * MAX_ROWS_FOR_TICK + self.row as u64) * speed as u64
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationTrack {
    pub id: u32,
    pub target: AutomationTarget,
    pub channel: Option<usize>,
    pub points: Vec<AutomationPoint>,
    pub default_interp: InterpolationMode,
    pub enabled: bool,
}

impl AutomationTrack {
    pub fn new(id: u32, target: AutomationTarget, channel: Option<usize>) -> Self {
        AutomationTrack {
            id,
            target,
            channel,
            points: Vec::new(),
            default_interp: InterpolationMode::Linear,
            enabled: true,
        }
    }

    pub fn label(&self) -> String {
        match self.channel {
            Some(ch) => format!("Ch{}: {}", ch + 1, self.target.label()),
            None => self.target.label().to_string(),
        }
    }

    pub fn insert_point(&mut self, point: AutomationPoint) {
        let key = (point.order, point.row);
        if let Some(existing) = self.points.iter().position(|p| (p.order, p.row) == key) {
            self.points[existing] = point;
        } else {
            self.points.push(point);
            self.points.sort_by_key(|p| (p.order, p.row));
        }
    }

    pub fn remove_point_at(&mut self, order: u16, row: u16) -> bool {
        let pos = self.points.iter().position(|p| p.order == order && p.row == row);
        match pos {
            Some(i) => {
                self.points.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn evaluate(&self, order: u16, row: u16, tick: u8, speed: u8) -> f32 {
        if self.points.is_empty() {
            return if self.target.is_multiplier() { 1.0 } else { 0.0 };
        }

        let target_tick = (order as u64 * MAX_ROWS_FOR_TICK + row as u64) * speed as u64
            + tick as u64;

        if target_tick <= self.points[0].song_tick(speed) {
            return self.points[0].value;
        }

        let last_idx = self.points.len() - 1;
        if target_tick >= self.points[last_idx].song_tick(speed) {
            return self.points[last_idx].value;
        }

        let right_idx = match self
            .points
            .binary_search_by_key(&target_tick, |p| p.song_tick(speed))
        {
            Ok(i) => return self.points[i].value,
            Err(i) => i,
        };

        let left_idx = right_idx - 1;
        let left = &self.points[left_idx];
        let right = &self.points[right_idx];

        let left_tick = left.song_tick(speed);
        let right_tick = right.song_tick(speed);
        let span = right_tick - left_tick;
        let t = if span > 0 {
            (target_tick - left_tick) as f64 / span as f64
        } else {
            0.0
        };

        match left.interp_to_next {
            InterpolationMode::Hold => left.value,
            InterpolationMode::Linear => left.value + (right.value - left.value) * t as f32,
            InterpolationMode::Smooth => {
                left.value + (right.value - left.value) * ((1.0 - (t * std::f64::consts::PI).cos()) / 2.0) as f32
            }
            InterpolationMode::Exponential => {
                if left.value.abs() < 1e-6 {
                    right.value * t as f32
                } else {
                    left.value * (right.value / left.value).powf(t as f32)
                }
            }
        }
    }
}

pub fn remap_automation_orders(
    tracks: &mut [AutomationTrack],
    at_order: u16,
    shift: i16,
) {
    for track in tracks.iter_mut() {
        for point in track.points.iter_mut() {
            if point.order >= at_order {
                let new_order = point.order as i16 + shift;
                point.order = if new_order < 0 { 0 } else { new_order as u16 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_point(order: u16, row: u16, value: f32, interp: InterpolationMode) -> AutomationPoint {
        AutomationPoint {
            order,
            row,
            value,
            interp_to_next: interp,
        }
    }

    fn make_track(points: Vec<AutomationPoint>) -> AutomationTrack {
        AutomationTrack {
            id: 0,
            target: AutomationTarget::ChannelVolume,
            channel: Some(0),
            points,
            default_interp: InterpolationMode::Linear,
            enabled: true,
        }
    }

    #[test]
    fn empty_track_returns_identity_for_multiplier() {
        let track = make_track(vec![]);
        assert_eq!(track.evaluate(0, 0, 0, 6), 1.0);
    }

    #[test]
    fn single_point_returns_its_value() {
        let track = make_track(vec![make_point(0, 8, 0.5, InterpolationMode::Hold)]);
        assert_eq!(track.evaluate(0, 8, 0, 6), 0.5);
        assert_eq!(track.evaluate(0, 4, 0, 6), 0.5);
        assert_eq!(track.evaluate(0, 16, 0, 6), 0.5);
    }

    #[test]
    fn hold_interpolation_snaps_to_left() {
        let track = make_track(vec![
            make_point(0, 0, 0.0, InterpolationMode::Hold),
            make_point(0, 8, 1.0, InterpolationMode::Hold),
        ]);
        assert_eq!(track.evaluate(0, 4, 0, 6), 0.0);
    }

    #[test]
    fn linear_interpolation_midpoint() {
        let track = make_track(vec![
            make_point(0, 0, 0.0, InterpolationMode::Linear),
            make_point(0, 8, 1.0, InterpolationMode::Linear),
        ]);
        let mid = track.evaluate(0, 4, 0, 6);
        assert!((mid - 0.5).abs() < 0.01);
    }

    #[test]
    fn linear_interpolation_sub_row() {
        let track = make_track(vec![
            make_point(0, 0, 0.0, InterpolationMode::Linear),
            make_point(0, 6, 1.0, InterpolationMode::Linear),
        ]);
        let val = track.evaluate(0, 3, 0, 6);
        assert!((val - 0.5).abs() < 0.01);
        let val_tick3 = track.evaluate(0, 3, 3, 6);
        assert!(val_tick3 > val);
    }

    #[test]
    fn smooth_interpolation_eases_at_endpoints() {
        let track = make_track(vec![
            make_point(0, 0, 0.0, InterpolationMode::Smooth),
            make_point(0, 8, 1.0, InterpolationMode::Smooth),
        ]);
        let at_quarter = track.evaluate(0, 2, 0, 6);
        let at_three_quarter = track.evaluate(0, 6, 0, 6);
        assert!(at_quarter < 0.5);
        assert!(at_three_quarter > 0.5);
    }

    #[test]
    fn exponential_interpolation() {
        let track = make_track(vec![
            make_point(0, 0, 1.0, InterpolationMode::Exponential),
            make_point(0, 8, 4.0, InterpolationMode::Exponential),
        ]);
        let mid = track.evaluate(0, 4, 0, 6);
        assert!((mid - 2.0).abs() < 0.01);
    }

    #[test]
    fn cross_order_interpolation() {
        let track = make_track(vec![
            make_point(0, 60, 0.0, InterpolationMode::Linear),
            make_point(1, 4, 1.0, InterpolationMode::Linear),
        ]);
        let val = track.evaluate(0, 62, 0, 6);
        assert!(val > 0.0 && val < 1.0);
    }

    #[test]
    fn insert_point_replaces_existing() {
        let mut track = make_track(vec![]);
        track.insert_point(make_point(0, 4, 0.5, InterpolationMode::Linear));
        assert_eq!(track.points.len(), 1);
        track.insert_point(make_point(0, 4, 0.8, InterpolationMode::Linear));
        assert_eq!(track.points.len(), 1);
        assert!((track.points[0].value - 0.8).abs() < 1e-6);
    }

    #[test]
    fn insert_point_maintains_sort() {
        let mut track = make_track(vec![]);
        track.insert_point(make_point(0, 8, 0.5, InterpolationMode::Linear));
        track.insert_point(make_point(0, 2, 0.3, InterpolationMode::Linear));
        assert_eq!(track.points[0].row, 2);
        assert_eq!(track.points[1].row, 8);
    }

    #[test]
    fn remove_point_at() {
        let mut track = make_track(vec![
            make_point(0, 4, 0.5, InterpolationMode::Linear),
            make_point(0, 8, 0.8, InterpolationMode::Linear),
        ]);
        assert!(track.remove_point_at(0, 4));
        assert_eq!(track.points.len(), 1);
        assert_eq!(track.points[0].row, 8);
        assert!(!track.remove_point_at(0, 99));
    }

    #[test]
    fn remap_orders_shifts_points() {
        let mut tracks = vec![make_track(vec![
            make_point(0, 0, 0.1, InterpolationMode::Linear),
            make_point(1, 0, 0.2, InterpolationMode::Linear),
            make_point(3, 0, 0.3, InterpolationMode::Linear),
        ])];
        remap_automation_orders(&mut tracks, 1, 2);
        assert_eq!(tracks[0].points[0].order, 0);
        assert_eq!(tracks[0].points[1].order, 3);
        assert_eq!(tracks[0].points[2].order, 5);
    }

    #[test]
    fn evaluate_before_first_point_returns_first_value() {
        let track = make_track(vec![make_point(2, 0, 0.75, InterpolationMode::Hold)]);
        assert_eq!(track.evaluate(0, 0, 0, 6), 0.75);
        assert_eq!(track.evaluate(1, 32, 0, 6), 0.75);
    }

    #[test]
    fn evaluate_after_last_point_holds_value() {
        let track = make_track(vec![
            make_point(0, 0, 0.2, InterpolationMode::Linear),
            make_point(0, 8, 0.6, InterpolationMode::Linear),
        ]);
        assert_eq!(track.evaluate(0, 16, 0, 6), 0.6);
        assert_eq!(track.evaluate(5, 0, 0, 6), 0.6);
    }
}
