//! MCP mutation dispatch.
//!
//! This module is the main-thread entry point for MCP tools that mutate
//! module/playback state. [`execute_mutation`] routes a dotted-string method
//! name to a handler in one of the per-domain submodules:
//!
//! | submodule    | domain                                            |
//! |--------------|---------------------------------------------------|
//! | [`common`]   | `parse_note` + typed-param extraction macros      |
//! | [`module`]   | `module.create` / `load` / `save`                 |
//! | [`order`]    | order-list editing                                |
//! | [`pattern`]  | pattern/cell editing + effect parsing             |
//! | [`transform`]| `pattern.transform` (batch transform ops)         |
//! | [`instrument`]| instrument create/remove/property/map           |
//! | [`sample`]   | sample load/remove/property + library import      |
//! | [`envelope`] | envelope set/add/remove/generate                  |
//! | [`automation`]| automation track create/remove/point/clear      |
//! | [`playback`] | transport control                                 |
//! | [`mixer`]    | channel panning/volume/mute/solo + send FX        |
//! | [`phrase`]   | `phrase.generate`                                 |
//! | [`history`]  | undo / redo                                       |
//!
//! Each handler has the uniform signature `fn(&mut HtrkCore, &serde_json::Value)
//! -> CmdResult` and follows the [`Module` mutation pattern][1] from
//! `AGENTS.md` §5 (`ensure_module_ownership` → `Arc::get_mut` → mutate →
//! `sync_module_to_audio`).
//!
//! [1]: ../../../../../AGENTS.md

use crate::core::HtrkCore;
use crate::mcp::protocol::CmdResult;

mod common;
mod automation;
mod envelope;
mod history;
mod instrument;
mod mixer;
mod module;
mod order;
mod pattern;
mod phrase;
mod playback;
mod sample;
mod transform;

// ── Entry point ──

pub fn execute_mutation(core: &mut HtrkCore, method: &str, params: &serde_json::Value) -> CmdResult {
    match method {
        // ── Module tools ──
        "module.create" => module::cmd_module_create(core, params),
        "module.load"   => module::cmd_module_load(core, params),
        "module.save"   => module::cmd_module_save(core, params),

        // ── Order list tools ──
        "order.set"     => order::cmd_order_set(core, params),
        "order.append"  => order::cmd_order_append(core, params),
        "order.insert"  => order::cmd_order_insert(core, params),
        "order.remove"  => order::cmd_order_remove(core, params),
        "order.set_entry" => order::cmd_order_set_entry(core, params),

        // ── Pattern / cell tools ──
        "pattern.ensure" => pattern::cmd_pattern_ensure(core, params),
        "cell.set"      => pattern::cmd_cell_set(core, params),
        "cell.set_batch"=> pattern::cmd_cell_set_batch(core, params),
        "pattern.fill"  => pattern::cmd_pattern_fill(core, params),
        "pattern.clear" => pattern::cmd_pattern_clear(core, params),
        "pattern.transpose" => pattern::cmd_pattern_transpose(core, params),
        "pattern.interpolate" => pattern::cmd_pattern_interpolate(core, params),

        // ── Instrument tools ──
        "instrument.create"    => instrument::cmd_instrument_create(core, params),
        "instrument.remove"    => instrument::cmd_instrument_remove(core, params),
        "instrument.set_property" => instrument::cmd_instrument_set_property(core, params),
        "instrument.map_note"  => instrument::cmd_instrument_map_note(core, params),
        "instrument.map_range" => instrument::cmd_instrument_map_range(core, params),

        // ── Sample tools ──
        "sample.load"          => sample::cmd_sample_load(core, params),
        "sample.remove"        => sample::cmd_sample_remove(core, params),
        "sample.set_property"  => sample::cmd_sample_set_property(core, params),
        "sample_library.import" => sample::cmd_sample_library_import(core, params),

        // ── Envelope tools ──
        "envelope.set"         => envelope::cmd_envelope_set(core, params),
        "envelope.add_point"   => envelope::cmd_envelope_add_point(core, params),
        "envelope.remove_point" => envelope::cmd_envelope_remove_point(core, params),
        "envelope.generate"    => envelope::cmd_envelope_generate(core, params),

        // ── Automation tools ──
        "automation.create"    => automation::cmd_automation_create(core, params),
        "automation.remove"    => automation::cmd_automation_remove(core, params),
        "automation.add_point" => automation::cmd_automation_add_point(core, params),
        "automation.clear"     => automation::cmd_automation_clear(core, params),

        // ── Playback tools ──
        "playback.play"        => playback::cmd_playback_play(core, params),
        "playback.stop"        => playback::cmd_playback_stop(core, params),
        "playback.set_position" => playback::cmd_playback_set_position(core, params),
        "playback.set_bpm"     => playback::cmd_playback_set_bpm(core, params),
        "playback.set_speed"   => playback::cmd_playback_set_speed(core, params),

        // ── Channel tools ──
        "channel.set_panning"  => mixer::cmd_channel_set_panning(core, params),
        "channel.set_volume"   => mixer::cmd_channel_set_volume(core, params),
        "channel.set_mute"     => mixer::cmd_channel_set_mute(core, params),
        "channel.set_solo"     => mixer::cmd_channel_set_solo(core, params),

        // ── Send FX tools ──
        "sendfx.set_bus"       => mixer::cmd_sendfx_set_bus(core, params),
        "sendfx.set_return_level" => mixer::cmd_sendfx_set_return_level(core, params),

        // ── Phrase generation / transforms / history ──
        "phrase.generate"      => phrase::cmd_phrase_generate(core, params),
        "pattern.transform"    => transform::cmd_pattern_transform(core, params),
        "undo.last"            => history::cmd_undo_last(core, params),
        "undo.to"              => history::cmd_undo_to(core, params),
        "redo.last"            => history::cmd_redo_last(core, params),

        _ => Err(format!("Unknown mutation tool: '{method}'")),
    }
}
