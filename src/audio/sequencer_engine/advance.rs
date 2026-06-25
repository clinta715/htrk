
use crate::sequencer::player::{ActiveEffects, PlayMode};
use crate::debug_log;
use crate::audio::sequencer_engine::SequencerEngine;

impl SequencerEngine {
    pub(crate) fn advance_envelopes(&mut self) {
        let is_xm = self.module.as_ref().map_or(false, |m| m.flags.xm_envelope_model);
        self.voice_pool.advance_envelopes(is_xm, &self.state, self.output_sample_rate, self.module.as_ref());
    }

    pub(crate) fn advance_row(&mut self) {
        self.state.clock.current_tick = 0;

        self.voice_pool.advance_row_voice_reset(self.use_xm_model, &self.state);

        for ch in &mut self.state.channels {
            ch.delayed_cell = None;
            ch.note_delay_ticks = 0;
            ch.active_effects = ActiveEffects::default();
            ch.last_retrigger_interval = 0;
            ch.retrig_speed = 0;
            ch.retrig_cnt = 0;
            ch.note_cut_tick = None;
            ch.vol_kol = 0;
        }

        if self.state.row_delay_active && self.state.pattern_delay_ticks > 0 {
            self.state.pattern_delay_ticks -= 1;
            return;
        }
        self.state.row_delay_active = false;
        self.state.pattern_delay_ticks = 0;

        let module = match self.module.as_ref() {
            Some(m) => m.clone(),
            None => {
                self.stop();
                return;
            }
        };

        if let Some(target_order) = self.state.position_jump_order.take() {
            if (target_order as usize) < module.order_list.len() {
                self.state.current_order = target_order;
                self.state.current_pattern = self.get_pattern_for_order(target_order);
                let target_row = self.state.pattern_break_row.take().unwrap_or(0);
                self.state.current_row = target_row;
                self.reset_pattern_loop_state();
                return;
            } else {
                self.stop();
                return;
            }
        }

        if let Some(target_row) = self.state.pattern_break_row.take() {
            self.state.current_order += 1;
            if (self.state.current_order as usize) >= module.order_list.len() {
                self.handle_song_end();
                return;
            }
            self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
            self.state.current_row = target_row;
            self.reset_pattern_loop_state();
            return;
        }

        // Handle Pattern Loop jump
        if let Some(target_row) = self.state.pattern_loop_jump_target.take() {
            if self.state.pattern_loop_count > 0 {
                self.state.current_row = target_row;
                self.state.pattern_loop_count -= 1;
                if self.state.pattern_loop_count == 0 {
                    self.state.pattern_loop_start = None;
                    self.state.pattern_loop_final_pass = true;
                }
            } else {
                self.state.pattern_loop_start = None;
                self.state.pattern_loop_final_pass = true;
            }
            return;
        }

        let pattern_idx = self.state.current_pattern as usize;
        let pattern_rows = if pattern_idx < module.patterns.len() {
            module.patterns[pattern_idx].num_rows
        } else {
            64
        };

        let next_row = self.state.current_row as usize + 1;
        if next_row >= pattern_rows {
            if self.state.pattern_loop_count > 0 {
                if let Some((_loop_order, loop_row)) = self.state.pattern_loop_start {
                    self.state.current_row = loop_row;
                    self.state.pattern_loop_count -= 1;
                    if self.state.pattern_loop_count == 0 {
                        self.state.pattern_loop_start = None;
                        self.state.pattern_loop_final_pass = true;
                    }
                    return;
                }
            }
            self.state.current_order += 1;
            if (self.state.current_order as usize) >= module.order_list.len() {
                self.handle_song_end();
                return;
            }
            self.state.current_pattern = self.get_pattern_for_order(self.state.current_order);
            self.state.current_row = 0;
            self.reset_pattern_loop_state();
        } else {
            self.state.current_row = next_row as u16;
        }
    }

    fn reset_pattern_loop_state(&mut self) {
        debug_log!("[LOOP] Resetting pattern loop state");
        self.state.pattern_loop_start = None;
        self.state.pattern_loop_count = 0;
        self.state.pattern_loop_final_pass = false;
        self.state.pattern_loop_jump_target = None;
    }

    pub(crate) fn handle_song_end(&mut self) {
        match self.state.play_mode {
            PlayMode::Once | PlayMode::Order => {
                self.stop();
            }
            PlayMode::Loop => {
                self.state.current_order = 0;
                self.state.current_row = 0;
                self.state.current_pattern = self.get_pattern_for_order(0);
            }
            PlayMode::Pattern => {
                self.state.current_row = 0;
            }
        }
    }

    pub(crate) fn get_pattern_for_order(&self, order: u16) -> u16 {
        let module = match self.module.as_ref() {
            Some(m) => m,
            None => return 0,
        };
        let order_idx = order as usize;
        if order_idx < module.order_list.len() {
            let pat_idx = module.order_list[order_idx] as usize;
            if pat_idx < module.patterns.len() {
                return pat_idx as u16;
            }
        }
        0
    }

    pub(crate) fn get_order_count(&self) -> usize {
        self.module.as_ref().map_or(0, |m| m.order_list.len())
    }
}
