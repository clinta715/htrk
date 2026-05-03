use std::{env, fs};
use htrk::formats::load_module;
use htrk::sequencer::{Module, Note, Effect, Cell};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: analyze_mod <file.mod>");
        return;
    }

    let data = match fs::read(&args[1]) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            return;
        }
    };

    let module = match load_module(&data) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to load module: {}", e);
            return;
        }
    };

    dump_header(&module);
    dump_samples(&module);
    dump_orders(&module);
}

fn effect_name(e: &Effect) -> String {
    match e {
        Effect::None => "None".into(),
        Effect::Arpeggio { note1, note2 } => format!("Arp({:X}{:X})", note1, note2),
        Effect::PortamentoUp { speed } => format!("PortaUp({})", speed),
        Effect::PortamentoDown { speed } => format!("PortaDown({})", speed),
        Effect::TonePortamento { speed } => format!("TonePorta({})", speed),
        Effect::Vibrato { speed, depth } => format!("Vibrato({:X}{:X})", speed, depth),
        Effect::VolumeSlide { up, down } => format!("VolSlide({:X}{:X})", up, down),
        Effect::PositionJump { order } => format!("PosJump({})", order),
        Effect::SetVolume { volume } => format!("SetVol({})", volume),
        Effect::PatternBreak { row } => format!("PatBreak({})", row),
        Effect::SetSpeed { speed } => format!("Speed({})", speed),
        Effect::SetTempo { bpm } => format!("Tempo({})", bpm),
        Effect::ExtendedEffect { param } => format!("Ext({:X}{:X})", param >> 4, param & 0xF),
        Effect::FinePortamentoUp { speed } => format!("FinePortaUp({})", speed),
        Effect::FinePortamentoDown { speed } => format!("FinePortaDown({})", speed),
        Effect::FineVolumeSlideUp { amount } => format!("FineVolUp({})", amount),
        Effect::FineVolumeSlideDown { amount } => format!("FineVolDown({})", amount),
        Effect::NoteCutAfter { ticks } => format!("NoteCut({})", ticks),
        Effect::NoteDelay { ticks } => format!("NoteDelay({})", ticks),
        Effect::PatternDelay { ticks } => format!("PatDelay({})", ticks),
        Effect::SetPanning { pan } => format!("Pan({})", pan),
        Effect::SetSampleOffset { offset } => format!("Offs({})", offset),
        Effect::SetGlobalVolume { volume } => format!("GlobVol({})", volume),
        Effect::TonePortamentoVolumeSlide { .. } => "TPorta+Vol".into(),
        Effect::VibratoVolumeSlide { .. } => "Vib+Vol".into(),
        Effect::Tremolo { speed, depth } => format!("Trem({:X}{:X})", speed, depth),
        Effect::GlissandoControl { on } => format!("Gliss({})", on),
        Effect::VibratoWaveform { waveform } => format!("VibWave({})", waveform),
        Effect::SetFineTune { tune } => format!("FineTune({})", tune),
        Effect::PatternLoop { count } => format!("PatLoop({})", count),
        Effect::TremoloWaveform { waveform } => format!("TremWave({})", waveform),
        Effect::SetPanning16 { pan } => format!("Pan16({})", pan),
        Effect::Retrigger { interval } => format!("Retrig({})", interval),
        _ => format!("{:?}", e),
    }
}

fn flag_warnings(module: &Module, ord_idx: usize, row: usize, ch: usize, cell: &Cell) {
    if let Effect::SetVolume { volume } = cell.effect {
        if volume == 0 {
            println!("        *** WARNING: SetVolume(0) at order {} row {} ch {} - channel silenced", ord_idx, row, ch);
        }
    }
    if let Effect::Arpeggio { note1, note2 } = cell.effect {
        if note1 == 0 && note2 == 0 {
            println!("        *** NOTE: Arpeggio(0,0) = no-op effect at order {} row {} ch {}", ord_idx, row, ch);
        }
    }
    if let Effect::VolumeSlide { up, down } = cell.effect {
        if up == 0 && down == 0 {
            println!("        *** NOTE: VolumeSlide(0,0) = no-op at order {} row {} ch {}", ord_idx, row, ch);
        }
    }
    if let Note::On(_) = cell.note {
        if let Some(inst) = cell.instrument {
            if inst as usize >= module.instruments.len() {
                println!("        *** WARNING: instrument {} >= instruments.len() {} at order {} row {} ch {}", inst, module.instruments.len(), ord_idx, row, ch);
            } else if inst > 0 {
                let inst_data = &module.instruments[inst as usize];
                let sample_idx = match cell.note {
                    Note::On(key) if (key as usize) < 120 => inst_data.sample_map[key as usize],
                    _ => 0,
                };
                if sample_idx == 0 {
                    println!("        *** WARNING: instrument {} maps note {} to sample 0 at order {} row {} ch {} (no sample for this key)", inst, cell.note, ord_idx, row, ch);
                } else if (sample_idx as usize) >= module.samples.len() {
                    println!("        *** WARNING: instrument {} maps to sample {} >= samples.len() {} at order {} row {} ch {}", inst, sample_idx, module.samples.len(), ord_idx, row, ch);
                } else if module.samples[sample_idx as usize].data.is_empty() {
                    println!("        *** WARNING: sample {} (from instrument {}) has empty data at order {} row {} ch {}", sample_idx, inst, ord_idx, row, ch);
                }
            }
        } else {
            println!("        *** WARNING: Note without instrument at order {} row {} ch {}", ord_idx, row, ch);
        }
    }
    match &cell.effect {
        Effect::ExtendedEffect { param } => {
            let sub = param >> 4;
            match sub {
                0x1 | 0x2 => println!("        *** NOTE: E{:X}{:X} = fine portamento (not regular) at order {} row {} ch {}", sub, param & 0xF, ord_idx, row, ch),
                0xA => println!("        *** NOTE: EA{:X} = FineVolumeSlideUp (not NoteCut) at order {} row {} ch {}", param & 0xF, ord_idx, row, ch),
                0xB => println!("        *** NOTE: EB{:X} = FineVolumeSlideDown (not NoteDelay) at order {} row {} ch {}", param & 0xF, ord_idx, row, ch),
                _ => {}
            }
        }
        _ => {}
    }
}

fn dump_header(module: &Module) {
    println!("=== Module Header ===");
    println!("Name:   {}", module.name);
    println!("Format: {:?}", module.format);
    println!("BPM:    {}  Speed: {}", module.initial_bpm, module.initial_speed);
    println!("Global Vol: {}", module.initial_global_volume);
    println!("Orders: {} entries, first = {:?}",
        module.order_list.len(),
        module.order_list.first().copied().unwrap_or(0));
    println!("Instruments: {} total", module.instruments.len());
    println!("Samples: {} total", module.samples.len());
    println!("Patterns: {} total", module.patterns.len());
    println!();
}

fn dump_samples(module: &Module) {
    println!("=== Samples (index 0 = empty/dummy) ===");
    for (i, s) in module.samples.iter().enumerate() {
        if i == 0 || s.data.is_empty() {
            continue;
        }
        let len_kb = s.data.len() as f64 / 1024.0;
        let loop_str = match s.loop_type {
            htrk::sequencer::LoopType::None => "None".to_string(),
            htrk::sequencer::LoopType::Forward => format!("Forward[{}-{}]", s.loop_start, s.loop_end),
            htrk::sequencer::LoopType::PingPong => format!("PingPong[{}-{}]", s.loop_start, s.loop_end),
            htrk::sequencer::LoopType::Backward => format!("Backward[{}-{}]", s.loop_start, s.loop_end),
        };
        println!("  {:2}: \"{}\"  {:>7.1}KB  {}Hz  vol={}  loop={}",
            i, s.name, len_kb, s.sample_rate, s.default_volume, loop_str);
    }
    println!();
}

fn dump_orders(module: &Module) {
    println!("=== Pattern Dump ===");
    println!("(showing only non-empty cells)");
    println!();

    for (ord_idx, &pat_idx) in module.order_list.iter().enumerate() {
        if pat_idx as usize >= module.patterns.len() {
            println!("[Order {}] *** INVALID pattern index {} (max {}) ***",
                ord_idx, pat_idx, module.patterns.len() - 1);
            continue;
        }

        let pattern = &module.patterns[pat_idx as usize];
        let mut has_any = false;

        for row in 0..pattern.num_rows {
            let mut row_cells = Vec::new();
            for ch in 0..64 {
                if ch >= pattern.data[row].len() {
                    break;
                }
                let cell = &pattern.data[row][ch];
                if cell.is_empty() {
                    continue;
                }
                row_cells.push((ch, cell));
            }

            if row_cells.is_empty() {
                continue;
            }

            if !has_any {
                println!("[Order {:3}] Pattern {} ({} rows)", ord_idx, pat_idx, pattern.num_rows);
                has_any = true;
            }

            for (ch, cell) in &row_cells {
                let note_str = format!("{}", cell.note);
                let inst_str = cell.instrument
                    .map(|i| format!("{:02X}", i))
                    .unwrap_or_else(|| "--".to_string());
                let eff_str = effect_name(&cell.effect);
                println!("  row={:3} ch={:2}  note={:>4}  inst={:>3}  eff={}",
                    row, ch, note_str, inst_str, eff_str);
                flag_warnings(module, ord_idx, row, *ch, cell);
            }
        }

        if !has_any {
            println!("[Order {:3}] Pattern {} (all empty)", ord_idx, pat_idx);
        }
    }
}
