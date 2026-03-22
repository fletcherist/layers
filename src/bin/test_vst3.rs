//! Real VST3 integration tests — runs on the main thread.
//!
//! Usage: cargo run --bin test_vst3
//!
//! Tests FabFilter Pro-Q 4 and FabFilter One directly (no plugin scan).

use std::path::Path;

struct TestRunner {
    passed: usize,
    failed: usize,
}

impl TestRunner {
    fn new() -> Self { Self { passed: 0, failed: 0 } }

    fn run(&mut self, name: &str, f: impl FnOnce() -> Result<(), String>) {
        print!("  {name} ... ");
        match f() {
            Ok(()) => { println!("\x1b[32mok\x1b[0m"); self.passed += 1; }
            Err(e) => { println!("\x1b[31mFAILED\x1b[0m: {e}"); self.failed += 1; }
        }
    }

    fn summary(&self) {
        println!();
        let color = if self.failed > 0 { "\x1b[31m" } else { "\x1b[32m" };
        println!("{color}result: {} passed, {} failed\x1b[0m", self.passed, self.failed);
    }

    fn exit_code(&self) -> i32 { if self.failed > 0 { 1 } else { 0 } }
}

fn open_hidden(path: &str, id: &str, name: &str) -> Option<vst3_gui::Vst3Gui> {
    let gui = vst3_gui::Vst3Gui::open(path, id, name)?;
    gui.hide();
    Some(gui)
}

fn main() {
    println!("=== VST3 Integration Tests ===\n");

    let mut t = TestRunner::new();

    // FabFilter Pro-Q 4
    let proq_path = "/Library/Audio/Plug-Ins/VST3/FabFilter Pro-Q 4.vst3";
    let proq_id = "ED57BD725C60467EA64DD2F400758B6F";
    let proq_name = "Pro-Q 4";

    if Path::new(proq_path).exists() {
        println!("Effect: {proq_name}");

        t.run("effect_open_with_gui", || {
            let gui = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            if gui.parameter_count() == 0 { return Err("0 parameters".into()); }
            Ok(())
        });

        t.run("effect_set_and_read_parameter", || {
            let gui = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            gui.set_parameter(0, 0.42);
            let val = gui.get_parameter(0).ok_or("get_parameter failed")?;
            if (val - 0.42).abs() > 0.05 {
                return Err(format!("expected ~0.42, got {val}"));
            }
            Ok(())
        });

        t.run("effect_state_save_restore", || {
            let gui1 = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            gui1.setup_processing(48000.0, 512);
            gui1.set_parameter(0, 0.75);
            let state = gui1.get_state().ok_or("get_state failed")?;
            let params = gui1.get_all_parameters();
            drop(gui1);

            let gui2 = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            gui2.setup_processing(48000.0, 512);
            gui2.set_state(&state);
            gui2.set_all_parameters(&params);
            let val = gui2.get_parameter(0).ok_or("get_parameter failed")?;
            if (val - 0.75).abs() > 0.05 {
                return Err(format!("expected ~0.75, got {val}"));
            }
            Ok(())
        });

        t.run("effect_audio_processing", || {
            let gui = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            gui.setup_processing(48000.0, 512);
            let input_l = vec![0.5f32; 512];
            let input_r = vec![0.5f32; 512];
            let mut output_l = vec![0.0f32; 512];
            let mut output_r = vec![0.0f32; 512];
            let inputs: Vec<&[f32]> = vec![&input_l, &input_r];
            let mut outputs: Vec<&mut [f32]> = vec![&mut output_l, &mut output_r];
            if !gui.process(&inputs, &mut outputs, 512) {
                return Err("process() failed".into());
            }
            if !output_l.iter().any(|&s| s != 0.0) && !output_r.iter().any(|&s| s != 0.0) {
                return Err("all-zero output".into());
            }
            Ok(())
        });

        t.run("effect_show_hide_cycle", || {
            let gui = open_hidden(proq_path, proq_id, proq_name).ok_or("open failed")?;
            if gui.is_open() { return Err("should be hidden".into()); }
            gui.show();
            if !gui.is_open() { return Err("should be visible after show".into()); }
            gui.hide();
            if gui.is_open() { return Err("should be hidden after hide".into()); }
            gui.set_parameter(0, 0.33);
            gui.show();
            gui.hide();
            let val = gui.get_parameter(0).ok_or("get failed")?;
            if (val - 0.33).abs() > 0.05 {
                return Err(format!("param didn't survive show/hide, got {val}"));
            }
            Ok(())
        });

        t.run("effect_multiple_instances", || {
            let gui1 = open_hidden(proq_path, proq_id, proq_name).ok_or("open1 failed")?;
            let gui2 = open_hidden(proq_path, proq_id, proq_name).ok_or("open2 failed")?;
            gui1.set_parameter(0, 0.2);
            gui2.set_parameter(0, 0.8);
            let v1 = gui1.get_parameter(0).ok_or("get1 failed")?;
            let v2 = gui2.get_parameter(0).ok_or("get2 failed")?;
            if (v1 - 0.2).abs() > 0.05 { return Err(format!("inst1: expected ~0.2, got {v1}")); }
            if (v2 - 0.8).abs() > 0.05 { return Err(format!("inst2: expected ~0.8, got {v2}")); }
            Ok(())
        });
    } else {
        println!("SKIP: {proq_name} not installed at {proq_path}");
    }

    // FabFilter One (instrument)
    let one_path = "/Library/Audio/Plug-Ins/VST3/FabFilter One.vst3";
    let one_id = "9240FC4010CD45A0B7DACECADDC2E97A";
    let one_name = "One";

    if Path::new(one_path).exists() {
        println!("\nInstrument: FabFilter {one_name}");

        t.run("instrument_open", || {
            let gui = open_hidden(one_path, one_id, one_name).ok_or("open failed")?;
            if gui.parameter_count() == 0 { return Err("0 parameters".into()); }
            Ok(())
        });

        t.run("instrument_midi_and_process", || {
            let gui = open_hidden(one_path, one_id, one_name).ok_or("open failed")?;
            gui.setup_processing(48000.0, 512);
            gui.send_midi_note_on(60, 100, 0, 0);
            let mut output_l = vec![0.0f32; 512];
            let mut output_r = vec![0.0f32; 512];
            let inputs: Vec<&[f32]> = vec![];
            let mut outputs: Vec<&mut [f32]> = vec![&mut output_l, &mut output_r];
            if !gui.process(&inputs, &mut outputs, 512) {
                return Err("process() failed".into());
            }
            if !output_l.iter().any(|&s| s != 0.0) && !output_r.iter().any(|&s| s != 0.0) {
                return Err("no audio after note-on".into());
            }
            Ok(())
        });
    } else {
        println!("\nSKIP: FabFilter {one_name} not installed at {one_path}");
    }

    t.summary();
    std::process::exit(t.exit_code());
}
