use crate::sequencer::instrument::{EnvelopeFlags, EnvelopePoint};
use crate::sequencer::pattern::Cell;
use std::sync::Arc;

#[derive(Debug)]
#[allow(dead_code)]
pub enum EditError {
    NoSelection,
    #[allow(dead_code)]
    CannotPasteDifferentChannels,
    PatternFull,
    #[allow(dead_code)]
    InvalidNoteValue,
}

pub trait EditCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError>;
    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError>;
    #[allow(dead_code)]
    fn description(&self) -> &str;
}

pub struct SetCellCommand {
    pub order: usize,
    pub row: usize,
    pub channel: usize,
    pub old_cell: Cell,
    pub new_cell: Cell,
}

impl EditCommand for SetCellCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        let pat_idx = *module.order_list.get(self.order).ok_or(EditError::NoSelection)? as usize;
        if pat_idx >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        module.patterns[pat_idx].data[self.row][self.channel] = self.new_cell;
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        let pat_idx = *module.order_list.get(self.order).ok_or(EditError::NoSelection)? as usize;
        if pat_idx >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        module.patterns[pat_idx].data[self.row][self.channel] = self.old_cell;
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Cell"
    }
}

pub struct InsertRowCommand {
    pub pattern_index: usize,
    pub row: usize,
    pub _channel: Option<usize>,
}

impl EditCommand for InsertRowCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.pattern_index >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        let pattern = &mut module.patterns[self.pattern_index];
        if pattern.num_rows >= crate::sequencer::module::MAX_PATTERN_ROWS {
            return Err(EditError::PatternFull);
        }
        pattern.data.insert(self.row, [Cell::default(); crate::sequencer::pattern::MAX_CHANNELS]);
        pattern.num_rows += 1;
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.pattern_index >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        let pattern = &mut module.patterns[self.pattern_index];
        if self.row < pattern.data.len() {
            pattern.data.remove(self.row);
            pattern.num_rows -= 1;
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Insert Row"
    }
}

pub struct DeleteRowCommand {
    pub pattern_index: usize,
    pub row: usize,
    pub _channel: Option<usize>,
    pub deleted_data: Vec<Cell>,
}

impl EditCommand for DeleteRowCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.pattern_index >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        let pattern = &mut module.patterns[self.pattern_index];
        if self.row < pattern.data.len() && pattern.num_rows > 1 {
            pattern.data.remove(self.row);
            pattern.num_rows -= 1;
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.pattern_index >= module.patterns.len() {
            return Err(EditError::NoSelection);
        }
        let pattern = &mut module.patterns[self.pattern_index];
        let mut row_data = [Cell::default(); crate::sequencer::pattern::MAX_CHANNELS];
        for (i, cell) in self.deleted_data.iter().enumerate() {
            if i < row_data.len() {
                row_data[i] = *cell;
            }
        }
        if self.row <= pattern.data.len() {
            pattern.data.insert(self.row, row_data);
            pattern.num_rows += 1;
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Delete Row"
    }
}

#[allow(dead_code)]
pub struct SetOrderEntryCommand {
    pub order_index: usize,
    pub old_pattern: u8,
    pub new_pattern: u8,
}

impl EditCommand for SetOrderEntryCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list[self.order_index] = self.new_pattern;
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list[self.order_index] = self.old_pattern;
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Order Entry"
    }
}

#[allow(dead_code)]
pub struct InsertOrderCommand {
    pub order_index: usize,
    pub pattern: u8,
}

impl EditCommand for InsertOrderCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if module.order_list.len() >= crate::sequencer::module::MAX_ORDER_LENGTH {
            return Err(EditError::PatternFull);
        }
        module.order_list.insert(self.order_index, self.pattern);
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.order_index < module.order_list.len() {
            module.order_list.remove(self.order_index);
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Insert Order"
    }
}

#[allow(dead_code)]
pub struct DeleteOrderCommand {
    pub order_index: usize,
    pub deleted_pattern: u8,
}

impl EditCommand for DeleteOrderCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list.remove(self.order_index);
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if module.order_list.len() < crate::sequencer::module::MAX_ORDER_LENGTH {
            module.order_list.insert(self.order_index, self.deleted_pattern);
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Delete Order"
    }
}

pub enum SampleProperty {
    Name(String),
    DefaultVolume(u8),
    DefaultPanning(u8),
    GlobalVolume(u8),
    LoopType(crate::sequencer::sample::LoopType),
    LoopStart(usize),
    LoopEnd(usize),
    RelativeNote(i8),
    FineTune(i8),
}

pub struct SetSamplePropertyCommand {
    pub sample_index: usize,
    pub property: SampleProperty,
    pub old_property: SampleProperty,
}

impl EditCommand for SetSamplePropertyCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        let sample = &mut module.samples[self.sample_index];
        match &self.property {
            SampleProperty::Name(n) => sample.name = n.clone(),
            SampleProperty::DefaultVolume(v) => sample.default_volume = *v,
            SampleProperty::DefaultPanning(p) => sample.default_panning = *p,
            SampleProperty::GlobalVolume(v) => sample.global_volume = *v,
            SampleProperty::LoopType(t) => sample.loop_type = *t,
            SampleProperty::LoopStart(s) => sample.loop_start = *s,
            SampleProperty::LoopEnd(e) => sample.loop_end = *e,
            SampleProperty::RelativeNote(n) => sample.relative_note = *n,
            SampleProperty::FineTune(t) => sample.fine_tune = *t,
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        let sample = &mut module.samples[self.sample_index];
        match &self.old_property {
            SampleProperty::Name(n) => sample.name = n.clone(),
            SampleProperty::DefaultVolume(v) => sample.default_volume = *v,
            SampleProperty::DefaultPanning(p) => sample.default_panning = *p,
            SampleProperty::GlobalVolume(v) => sample.global_volume = *v,
            SampleProperty::LoopType(t) => sample.loop_type = *t,
            SampleProperty::LoopStart(s) => sample.loop_start = *s,
            SampleProperty::LoopEnd(e) => sample.loop_end = *e,
            SampleProperty::RelativeNote(n) => sample.relative_note = *n,
            SampleProperty::FineTune(t) => sample.fine_tune = *t,
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Sample Property"
    }
}

pub struct MapNoteToSampleCommand {
    pub instrument_index: usize,
    pub note: u8,
    pub old_sample: u8,
    pub new_sample: u8,
}

impl EditCommand for MapNoteToSampleCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].sample_map[self.note as usize] = self.new_sample;
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].sample_map[self.note as usize] = self.old_sample;
        Ok(())
    }

    fn description(&self) -> &str {
        "Map Note to Sample"
    }
}

pub struct MapNoteToNoteCommand {
    pub instrument_index: usize,
    pub note: u8,
    pub old_dest: u8,
    pub new_dest: u8,
}

impl EditCommand for MapNoteToNoteCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 || self.new_dest >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].note_map[self.note as usize] = self.new_dest;
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].note_map[self.note as usize] = self.old_dest;
        Ok(())
    }

    fn description(&self) -> &str {
        "Map Note to Note"
    }
}

pub struct SetSampleMapCommand {
    pub instrument_index: usize,
    pub new_sample_index: u8,
    pub old_map: [u8; 120],
}

impl EditCommand for SetSampleMapCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        for i in 0..120 {
            inst.sample_map[i] = self.new_sample_index;
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        inst.sample_map = self.old_map;
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Sample Map"
    }
}

pub enum InstrumentProperty {
    Name(String),
    Nna(crate::sequencer::instrument::NewNoteAction),
    DuplicateCheckType(crate::sequencer::instrument::DuplicateCheckType),
    DuplicateCheckAction(crate::sequencer::instrument::DuplicateCheckAction),
    Fadeout(u16),
    GlobalVolume(u8),
    PitchPanSeparation(i8),
    PitchPanCenter(u8),
    RandomVolume(u8),
    RandomPanning(u8),
    FilterCutoff(u16),
    FilterResonance(u8),
    FilterType(crate::sequencer::effect::FilterType),
}

pub struct SetInstrumentPropertyCommand {
    pub instrument_index: usize,
    pub property: InstrumentProperty,
    pub old_property: InstrumentProperty,
}

impl EditCommand for SetInstrumentPropertyCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        match &self.property {
            InstrumentProperty::Name(n) => inst.name = n.clone(),
            InstrumentProperty::Nna(n) => inst.nna = *n,
            InstrumentProperty::DuplicateCheckType(t) => inst.duplicate_check_type = *t,
            InstrumentProperty::DuplicateCheckAction(a) => inst.duplicate_check_action = *a,
            InstrumentProperty::Fadeout(f) => inst.fade_out = *f,
            InstrumentProperty::GlobalVolume(v) => inst.global_volume = *v,
            InstrumentProperty::PitchPanSeparation(s) => inst.pitch_pan_separation = *s,
            InstrumentProperty::PitchPanCenter(c) => inst.pitch_pan_center = *c,
            InstrumentProperty::RandomVolume(v) => inst.random_volume = *v,
            InstrumentProperty::RandomPanning(p) => inst.random_panning = *p,
            InstrumentProperty::FilterCutoff(c) => inst.filter_cutoff = *c,
            InstrumentProperty::FilterResonance(r) => inst.filter_resonance = *r,
            InstrumentProperty::FilterType(t) => inst.filter_type = *t,
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        match &self.old_property {
            InstrumentProperty::Name(n) => inst.name = n.clone(),
            InstrumentProperty::Nna(n) => inst.nna = *n,
            InstrumentProperty::DuplicateCheckType(t) => inst.duplicate_check_type = *t,
            InstrumentProperty::DuplicateCheckAction(a) => inst.duplicate_check_action = *a,
            InstrumentProperty::Fadeout(f) => inst.fade_out = *f,
            InstrumentProperty::GlobalVolume(v) => inst.global_volume = *v,
            InstrumentProperty::PitchPanSeparation(s) => inst.pitch_pan_separation = *s,
            InstrumentProperty::PitchPanCenter(c) => inst.pitch_pan_center = *c,
            InstrumentProperty::RandomVolume(v) => inst.random_volume = *v,
            InstrumentProperty::RandomPanning(p) => inst.random_panning = *p,
            InstrumentProperty::FilterCutoff(c) => inst.filter_cutoff = *c,
            InstrumentProperty::FilterResonance(r) => inst.filter_resonance = *r,
            InstrumentProperty::FilterType(t) => inst.filter_type = *t,
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Instrument Property"
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnvelopeType {
    Volume,
    Panning,
    Pitch,
    Filter,
}

pub struct SetSampleDataCommand {
    pub sample_index: usize,
    pub old_data: Arc<Vec<f32>>,
    pub new_data: Arc<Vec<f32>>,
}

impl EditCommand for SetSampleDataCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        module.samples[self.sample_index].data = self.new_data.clone();
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        module.samples[self.sample_index].data = self.old_data.clone();
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Sample Data"
    }
}

pub struct AddEnvelopePointCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub point: EnvelopePoint,
}

impl EditCommand for AddEnvelopePointCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.points.push(self.point);
            env.points.sort_by_key(|p| p.tick);
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            if let Some(pos) = env.points.iter().position(|p| p.tick == self.point.tick && p.value == self.point.value) {
                env.points.remove(pos);
            }
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Add Envelope Point"
    }
}

pub struct RemoveEnvelopePointCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub point_index: usize,
    pub old_point: EnvelopePoint,
}

impl EditCommand for RemoveEnvelopePointCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            if self.point_index < env.points.len() {
                env.points.remove(self.point_index);
            }
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.points.insert(self.point_index, self.old_point);
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Remove Envelope Point"
    }
}

pub struct SetEnvelopePointCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub point_index: usize,
    pub old_point: crate::sequencer::instrument::EnvelopePoint,
    pub new_point: crate::sequencer::instrument::EnvelopePoint,
}

impl EditCommand for SetEnvelopePointCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            if self.point_index < env.points.len() {
                env.points[self.point_index] = self.new_point;
            }
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            if self.point_index < env.points.len() {
                env.points[self.point_index] = self.old_point;
            }
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Envelope Point"
    }
}

pub struct SetEnvelopeSustainCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub old_sustain: Option<usize>,
    pub new_sustain: Option<usize>,
}

impl EditCommand for SetEnvelopeSustainCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.sustain_point = self.new_sustain;
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.sustain_point = self.old_sustain;
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Envelope Sustain"
    }
}

pub struct SetEnvelopeLoopCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub old_loop_enabled: bool,
    pub new_loop_enabled: bool,
    pub old_loop_start: Option<usize>,
    pub new_loop_start: Option<usize>,
    pub old_loop_end: Option<usize>,
    pub new_loop_end: Option<usize>,
}

impl EditCommand for SetEnvelopeLoopCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.flags.loop_ = self.new_loop_enabled;
            env.loop_start = self.new_loop_start;
            env.loop_end = self.new_loop_end;
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.flags.loop_ = self.old_loop_enabled;
            env.loop_start = self.old_loop_start;
            env.loop_end = self.old_loop_end;
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Envelope Loop"
    }
}

pub struct SetEnvelopeFlagsCommand {
    pub instrument_index: usize,
    pub envelope_type: EnvelopeType,
    pub old_flags: EnvelopeFlags,
    pub new_flags: EnvelopeFlags,
}

impl EditCommand for SetEnvelopeFlagsCommand {
    fn execute(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.flags = self.new_flags;
        }
        Ok(())
    }

    fn undo(&self, module: &mut crate::sequencer::Module) -> Result<(), EditError> {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(env) = envelope {
            env.flags = self.old_flags;
        }
        Ok(())
    }

    fn description(&self) -> &str {
        "Set Envelope Flags"
    }
}
