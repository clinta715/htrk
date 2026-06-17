use crate::sequencer::instrument::{EnvelopeFlags, EnvelopePoint};
use crate::sequencer::note::Note;
use crate::sequencer::pattern::{Cell, Pattern};
use std::sync::Arc;

fn ensure_pattern_by_index(module: &mut crate::sequencer::Module, pat_idx: usize) {
    if pat_idx >= module.patterns.len() {
        module.patterns.resize_with(pat_idx + 1, || Pattern::new(64));
    }
}

fn ensure_pattern(module: &mut crate::sequencer::Module, order: usize) -> Result<usize, EditError> {
    let pat_idx = *module.order_list.get(order).ok_or(EditError::NoSelection)? as usize;
    ensure_pattern_by_index(module, pat_idx);
    Ok(pat_idx)
}

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

macro_rules! edit_cmd {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($field:ident: $type:ty),* $(,)?
        }
        desc = $desc:expr;
        execute($s:ident, $m:ident) $exec:block
        undo($us:ident, $um:ident) $undo:block
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(pub $field: $type),*
        }
        impl EditCommand for $name {
            fn execute(&$s, $m: &mut crate::sequencer::Module) -> Result<(), EditError> {
                $exec
            }
            fn undo(&$us, $um: &mut crate::sequencer::Module) -> Result<(), EditError> {
                $undo
            }
            fn description(&self) -> &str {
                $desc
            }
        }
    };
}

edit_cmd! {
    pub struct SetCellCommand {
        order: usize,
        row: usize,
        channel: usize,
        old_cell: Cell,
        new_cell: Cell,
    }
    desc = "Set Cell";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        if self.row >= module.patterns[pat_idx].num_rows || self.channel >= crate::sequencer::pattern::MAX_CHANNELS {
            return Err(EditError::NoSelection);
        }
        module.patterns[pat_idx].data[self.row][self.channel] = self.new_cell;
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        if self.row >= module.patterns[pat_idx].num_rows || self.channel >= crate::sequencer::pattern::MAX_CHANNELS {
            return Err(EditError::NoSelection);
        }
        module.patterns[pat_idx].data[self.row][self.channel] = self.old_cell;
        Ok(())
    }
}

edit_cmd! {
    pub struct InsertRowCommand {
        pattern_index: usize,
        row: usize,
        _channel: Option<usize>,
    }
    desc = "Insert Row";
    execute(self, module) {
        ensure_pattern_by_index(module, self.pattern_index);
        let pattern = &mut module.patterns[self.pattern_index];
        if pattern.num_rows >= crate::sequencer::module::MAX_PATTERN_ROWS {
            return Err(EditError::PatternFull);
        }
        pattern.data.insert(self.row, [Cell::default(); crate::sequencer::pattern::MAX_CHANNELS]);
        pattern.num_rows += 1;
        Ok(())
    }
    undo(self, module) {
        ensure_pattern_by_index(module, self.pattern_index);
        let pattern = &mut module.patterns[self.pattern_index];
        if self.row < pattern.data.len() {
            pattern.data.remove(self.row);
            pattern.num_rows -= 1;
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct DeleteRowCommand {
        pattern_index: usize,
        row: usize,
        _channel: Option<usize>,
        deleted_data: Vec<Cell>,
    }
    desc = "Delete Row";
    execute(self, module) {
        ensure_pattern_by_index(module, self.pattern_index);
        let pattern = &mut module.patterns[self.pattern_index];
        if self.row < pattern.data.len() && pattern.num_rows > 1 {
            pattern.data.remove(self.row);
            pattern.num_rows -= 1;
        }
        Ok(())
    }
    undo(self, module) {
        ensure_pattern_by_index(module, self.pattern_index);
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
}

edit_cmd! {
    #[allow(dead_code)]
    pub struct SetOrderEntryCommand {
        order_index: usize,
        old_pattern: u8,
        new_pattern: u8,
    }
    desc = "Set Order Entry";
    execute(self, module) {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list[self.order_index] = self.new_pattern;
        Ok(())
    }
    undo(self, module) {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list[self.order_index] = self.old_pattern;
        Ok(())
    }
}

edit_cmd! {
    #[allow(dead_code)]
    pub struct InsertOrderCommand {
        order_index: usize,
        pattern: u8,
    }
    desc = "Insert Order";
    execute(self, module) {
        if module.order_list.len() >= crate::sequencer::module::MAX_ORDER_LENGTH {
            return Err(EditError::PatternFull);
        }
        module.order_list.insert(self.order_index, self.pattern);
        Ok(())
    }
    undo(self, module) {
        if self.order_index < module.order_list.len() {
            module.order_list.remove(self.order_index);
        }
        Ok(())
    }
}

edit_cmd! {
    #[allow(dead_code)]
    pub struct DeleteOrderCommand {
        order_index: usize,
        deleted_pattern: u8,
    }
    desc = "Delete Order";
    execute(self, module) {
        if self.order_index >= module.order_list.len() {
            return Err(EditError::NoSelection);
        }
        module.order_list.remove(self.order_index);
        Ok(())
    }
    undo(self, module) {
        if module.order_list.len() < crate::sequencer::module::MAX_ORDER_LENGTH {
            module.order_list.insert(self.order_index, self.deleted_pattern);
        }
        Ok(())
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

edit_cmd! {
    pub struct SetSamplePropertyCommand {
        sample_index: usize,
        property: SampleProperty,
        old_property: SampleProperty,
    }
    desc = "Set Sample Property";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct MapNoteToSampleCommand {
        instrument_index: usize,
        note: u8,
        old_sample: u8,
        new_sample: u8,
    }
    desc = "Map Note to Sample";
    execute(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].sample_map[self.note as usize] = self.new_sample;
        Ok(())
    }
    undo(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].sample_map[self.note as usize] = self.old_sample;
        Ok(())
    }
}

edit_cmd! {
    pub struct MapNoteToNoteCommand {
        instrument_index: usize,
        note: u8,
        old_dest: u8,
        new_dest: u8,
    }
    desc = "Map Note to Note";
    execute(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 || self.new_dest >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].note_map[self.note as usize] = self.new_dest;
        Ok(())
    }
    undo(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        if self.note >= 120 {
            return Err(EditError::InvalidNoteValue);
        }
        module.instruments[self.instrument_index].note_map[self.note as usize] = self.old_dest;
        Ok(())
    }
}

edit_cmd! {
    pub struct SetSampleMapCommand {
        instrument_index: usize,
        new_sample_index: u8,
        old_map: [u8; 120],
    }
    desc = "Set Sample Map";
    execute(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        for i in 0..120 {
            inst.sample_map[i] = self.new_sample_index;
        }
        Ok(())
    }
    undo(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let inst = &mut module.instruments[self.instrument_index];
        inst.sample_map = self.old_map;
        Ok(())
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
    FilterRandomCutoff(u8),
    VibType(u8),
    VibSweep(u8),
    VibDepth(u8),
    VibRate(u8),
}

edit_cmd! {
    pub struct SetInstrumentPropertyCommand {
        instrument_index: usize,
        property: InstrumentProperty,
        old_property: InstrumentProperty,
    }
    desc = "Set Instrument Property";
    execute(self, module) {
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
            InstrumentProperty::FilterRandomCutoff(c) => inst.filter_random_cutoff = *c,
            InstrumentProperty::VibType(v) => inst.vib_type = *v,
            InstrumentProperty::VibSweep(v) => inst.vib_sweep = *v,
            InstrumentProperty::VibDepth(v) => inst.vib_depth = *v,
            InstrumentProperty::VibRate(v) => inst.vib_rate = *v,
        }
        Ok(())
    }
    undo(self, module) {
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
            InstrumentProperty::FilterRandomCutoff(c) => inst.filter_random_cutoff = *c,
            InstrumentProperty::VibType(v) => inst.vib_type = *v,
            InstrumentProperty::VibSweep(v) => inst.vib_sweep = *v,
            InstrumentProperty::VibDepth(v) => inst.vib_depth = *v,
            InstrumentProperty::VibRate(v) => inst.vib_rate = *v,
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnvelopeType {
    Volume,
    Panning,
    Pitch,
    Filter,
}

edit_cmd! {
    pub struct SetSampleDataCommand {
        sample_index: usize,
        old_data: Arc<Vec<f32>>,
        new_data: Arc<Vec<f32>>,
    }
    desc = "Set Sample Data";
    execute(self, module) {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        module.samples[self.sample_index].data = self.new_data.clone();
        Ok(())
    }
    undo(self, module) {
        if self.sample_index >= module.samples.len() {
            return Err(EditError::NoSelection);
        }
        module.samples[self.sample_index].data = self.old_data.clone();
        Ok(())
    }
}

edit_cmd! {
    pub struct AddEnvelopePointCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        point: EnvelopePoint,
    }
    desc = "Add Envelope Point";
    execute(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if envelope.is_none() {
            *envelope = Some(crate::sequencer::instrument::Envelope {
                points: Vec::new(),
                sustain_point: None,
                loop_start: None,
                loop_end: None,
                flags: crate::sequencer::instrument::EnvelopeFlags {
                    enabled: true, sustain: false, loop_: false, carry: false,
                },
            });
        }
        if let Some(env) = envelope {
            env.points.push(self.point);
            env.points.sort_by_key(|p| p.tick);
        }
        Ok(())
    }
    undo(self, module) {
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
}

edit_cmd! {
    pub struct RemoveEnvelopePointCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        point_index: usize,
        old_point: EnvelopePoint,
    }
    desc = "Remove Envelope Point";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct SetEnvelopePointCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        point_index: usize,
        old_point: crate::sequencer::instrument::EnvelopePoint,
        new_point: crate::sequencer::instrument::EnvelopePoint,
    }
    desc = "Set Envelope Point";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct SetEnvelopeSustainCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        old_sustain: Option<usize>,
        new_sustain: Option<usize>,
    }
    desc = "Set Envelope Sustain";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct SetEnvelopeLoopCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        old_loop_enabled: bool,
        new_loop_enabled: bool,
        old_loop_start: Option<usize>,
        new_loop_start: Option<usize>,
        old_loop_end: Option<usize>,
        new_loop_end: Option<usize>,
    }
    desc = "Set Envelope Loop";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct SetEnvelopePointsCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        new_points: Vec<EnvelopePoint>,
        old_points: Vec<EnvelopePoint>,
        old_envelope: Option<crate::sequencer::instrument::Envelope>,
    }
    desc = "Set Envelope Points";
    execute(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if envelope.is_none() {
            *envelope = Some(crate::sequencer::instrument::Envelope {
                points: self.new_points.clone(),
                sustain_point: None,
                loop_start: None,
                loop_end: None,
                flags: crate::sequencer::instrument::EnvelopeFlags {
                    enabled: true, sustain: false, loop_: false, carry: false,
                },
            });
        } else if let Some(env) = envelope {
            env.points = self.new_points.clone();
        }
        Ok(())
    }
    undo(self, module) {
        if self.instrument_index >= module.instruments.len() {
            return Err(EditError::NoSelection);
        }
        let envelope = match self.envelope_type {
            EnvelopeType::Volume => &mut module.instruments[self.instrument_index].volume_envelope,
            EnvelopeType::Panning => &mut module.instruments[self.instrument_index].panning_envelope,
            EnvelopeType::Pitch => &mut module.instruments[self.instrument_index].pitch_envelope,
            EnvelopeType::Filter => &mut module.instruments[self.instrument_index].filter_envelope,
        };
        if let Some(old_env) = &self.old_envelope {
            *envelope = Some(old_env.clone());
        } else if let Some(env) = envelope {
            env.points = self.old_points.clone();
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct SetEnvelopeFlagsCommand {
        instrument_index: usize,
        envelope_type: EnvelopeType,
        old_flags: EnvelopeFlags,
        new_flags: EnvelopeFlags,
    }
    desc = "Set Envelope Flags";
    execute(self, module) {
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
    undo(self, module) {
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
}

edit_cmd! {
    pub struct TransposeCommand {
        order: usize,
        delta: i8,
        old_notes: Vec<(usize, usize, Note)>,
    }
    desc = "Transpose";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for &(row, ch, note) in &self.old_notes {
            if let Note::On(key) = note {
                let new_key = (key as i8 + self.delta).max(0).min(119) as u8;
                module.patterns[pat_idx].data[row][ch].note = Note::On(new_key);
            }
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for &(row, ch, note) in &self.old_notes {
            module.patterns[pat_idx].data[row][ch].note = note;
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct FillInstrumentCommand {
        order: usize,
        old_cells: Vec<(usize, usize, Cell)>,
        instrument: u8,
    }
    desc = "Fill Instrument";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for &(row, ch, _) in &self.old_cells {
            let cell = &mut module.patterns[pat_idx].data[row][ch];
            if cell.note != Note::None {
                cell.instrument = Some(self.instrument);
            }
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (row, ch, old_cell) in &self.old_cells {
            module.patterns[pat_idx].data[*row][*ch] = *old_cell;
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct InterpolateCommand {
        order: usize,
        old_cells: Vec<(usize, usize, Cell)>,
        new_cells: Vec<(usize, usize, Cell)>,
    }
    desc = "Interpolate";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (row, ch, cell) in &self.new_cells {
            module.patterns[pat_idx].data[*row][*ch] = *cell;
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (row, ch, cell) in &self.old_cells {
            module.patterns[pat_idx].data[*row][*ch] = *cell;
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct BulkSetCellsCommand {
        order: usize,
        old_cells: Vec<(usize, usize, Cell)>,
        new_cells: Vec<(usize, usize, Cell)>,
    }
    desc = "Bulk Set Cells";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        let num_rows = module.patterns[pat_idx].num_rows;
        for &(row, ch, cell) in &self.new_cells {
            if row < num_rows && ch < crate::sequencer::pattern::MAX_CHANNELS {
                module.patterns[pat_idx].data[row][ch] = cell;
            }
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        let num_rows = module.patterns[pat_idx].num_rows;
        for &(row, ch, cell) in &self.old_cells {
            if row < num_rows && ch < crate::sequencer::pattern::MAX_CHANNELS {
                module.patterns[pat_idx].data[row][ch] = cell;
            }
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct ReverseCommand {
        order: usize,
        channel: usize,
        start_row: usize,
        end_row: usize,
        old_cells: Vec<Cell>,
    }
    desc = "Reverse";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        let mut cells: Vec<Cell> = (self.start_row..=self.end_row)
            .map(|r| module.patterns[pat_idx].data[r][self.channel])
            .collect();
        cells.reverse();
        for (i, r) in (self.start_row..=self.end_row).enumerate() {
            module.patterns[pat_idx].data[r][self.channel] = cells[i];
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (i, r) in (self.start_row..=self.end_row).enumerate() {
            module.patterns[pat_idx].data[r][self.channel] = self.old_cells[i];
        }
        Ok(())
    }
}

edit_cmd! {
    pub struct RandomizeCommand {
        order: usize,
        old_cells: Vec<(usize, usize, Cell)>,
        new_cells: Vec<(usize, usize, Cell)>,
    }
    desc = "Randomize";
    execute(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (row, ch, cell) in &self.new_cells {
            module.patterns[pat_idx].data[*row][*ch] = *cell;
        }
        Ok(())
    }
    undo(self, module) {
        let pat_idx = ensure_pattern(module, self.order)?;
        for (row, ch, cell) in &self.old_cells {
            module.patterns[pat_idx].data[*row][*ch] = *cell;
        }
        Ok(())
    }
}
