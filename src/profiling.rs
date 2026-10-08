//! Opt-in draw/submission capture. Never labels platform submission as scanout.
use gpui_kit::{
    App, KeyBinding, actions,
    profiler::{self, FrameEvent, FrameTimingCollector},
};
use std::{
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

actions!(diagnostics, [CaptureFrames]);
static RECORDING: AtomicBool = AtomicBool::new(false);

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("ctrl-alt-p", CaptureFrames, None)]);
    cx.on_action(|_: &CaptureFrames, _| {
        if RECORDING.swap(true, Ordering::SeqCst) {
            return;
        }
        let collector = FrameTimingCollector::new();
        profiler::set_trace_enabled(true);
        std::thread::spawn(move || {
            let result = capture(collector);
            profiler::set_trace_enabled(false);
            RECORDING.store(false, Ordering::SeqCst);
            if let Err(error) = result {
                eprintln!("Frame capture failed: {error}");
            }
        });
    });
}

fn capture(mut collector: FrameTimingCollector) -> std::io::Result<()> {
    let started = Instant::now();
    let mut events = Vec::with_capacity(8192);
    // Bounded capture and no disk writes on the UI thread. Poll well below the
    // framework ring capacity even on a 120Hz display with multiple windows.
    while started.elapsed() < Duration::from_secs(30) {
        events.extend(collector.collect_unseen());
        if events.len() > 30000 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    events.extend(collector.collect_unseen());
    let directory = std::env::var_os("CANOPY_PROFILE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("canopy-profiles"));
    std::fs::create_dir_all(&directory)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = directory.join(format!("frames-{stamp}.csv"));
    let mut file = std::io::BufWriter::new(std::fs::File::create(&path)?);
    writeln!(
        file,
        "window,event,elapsed_ms,work_ms,dirty_to_draw_ms,submission_ms,animation_submission_interval_ms"
    )?;
    for event in events {
        match event {
            FrameEvent::Draw(frame) => writeln!(
                file,
                "{:?},draw,{:.6},{:.6},{},,",
                frame.window_id,
                frame
                    .draw_start
                    .saturating_duration_since(started)
                    .as_secs_f64()
                    * 1000.,
                frame.draw_duration().as_secs_f64() * 1000.,
                frame
                    .dirty_to_draw_duration()
                    .map(|d| format!("{:.6}", d.as_secs_f64() * 1000.))
                    .unwrap_or_default()
            )?,
            FrameEvent::Present(frame) => writeln!(
                file,
                "{:?},submit,{:.6},,,{:.6},{}",
                frame.window_id,
                frame
                    .present_start
                    .saturating_duration_since(started)
                    .as_secs_f64()
                    * 1000.,
                frame.present_duration().as_secs_f64() * 1000.,
                frame
                    .animation_interval
                    .map(|d| format!("{:.6}", d.as_secs_f64() * 1000.))
                    .unwrap_or_default()
            )?,
        }
    }
    file.flush()?;
    eprintln!("Frame capture: {}", path.display());
    Ok(())
}
