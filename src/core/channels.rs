use crate::audio::commands::AudioCommand;
use crate::sequencer::effect::NUM_SEND_BUSES;

use super::HtrkCore;

impl HtrkCore {
    pub fn toggle_mute(&mut self, channel: usize) {
        if channel < self.muted_channels.len() {
            self.muted_channels[channel] = !self.muted_channels[channel];
            self.send_command(AudioCommand::SetChannelMuted {
                channel,
                muted: self.muted_channels[channel],
            });
        }
    }

    pub fn toggle_solo(&mut self, channel: usize) {
        if channel < self.solo_channels.len() {
            self.solo_channels[channel] = !self.solo_channels[channel];
            self.send_command(AudioCommand::SetChannelSolo {
                channel,
                solo: self.solo_channels[channel],
            });
        }
    }

    pub fn set_send_level(&mut self, channel: usize, send_index: usize, level: f32) {
        if channel < self.send_levels.len() && send_index < NUM_SEND_BUSES {
            self.send_levels[channel][send_index] = level;
            self.send_command(AudioCommand::SetSendLevel { channel, send_index, level });
        }
    }

    pub fn add_channel(&mut self) {
        if let Some(ref module) = self.module {
            let count = module.channel_panning.len();
            self.send_levels.resize(count + 1, [0.0; NUM_SEND_BUSES]);
            self.muted_channels.resize(count + 1, false);
            self.solo_channels.resize(count + 1, false);
            self.automation_targets.resize(count + 1, None);
        }
    }

    pub fn remove_channel(&mut self) {
        if let Some(ref module) = self.module {
            let count = module.channel_panning.len();
            if count > 0 {
                self.send_levels.resize(count - 1, [0.0; NUM_SEND_BUSES]);
                self.muted_channels.resize(count - 1, false);
                self.solo_channels.resize(count - 1, false);
            }
        }
    }
}