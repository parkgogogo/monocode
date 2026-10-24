//! A frame readout for profiling: launched with `MONOCODE_FRAME_LOG=<file>`,
//! the app appends its repaint rate to that file once a second.
//! `MONOCODE_FRAME_LOG=/tmp/monocode-frames.log npm run native:dev`
//!
//! `renders` counts real window renders — each one hands a frame to the
//! renderer — so it is the number the user would call the draw rate.
//! `notifies` counts the views the shared clock asked to repaint, and `ticks`
//! the clock's own 33ms steps, which stop once no loop is left to draw. The
//! `leases` list names the views still driving a loop together with the call
//! site that leased them, so an app that never parks its clock can be traced
//! back to the element keeping it awake. Compare runs with the window
//! frontmost and behind another app to see the background stride.
use gpui::App;
use std::{
    io::Write,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const PERIOD: Duration = Duration::from_secs(1);

static RENDERS: AtomicU64 = AtomicU64::new(0);

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("MONOCODE_FRAME_LOG").is_some())
}

/// Called from the root view's render: one call, one frame.
pub(crate) fn count_render() {
    if enabled() {
        RENDERS.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn start(cx: &mut App) {
    let Some(path) = std::env::var_os("MONOCODE_FRAME_LOG") else {
        return;
    };
    let executor = cx.background_executor().clone();
    cx.spawn(async move |cx| {
        let mut last_tick = 0;
        loop {
            executor.timer(PERIOD).await;
            let line = cx.update(|cx| {
                let snapshot = crate::pulse::snapshot(cx);
                let renders = RENDERS.swap(0, Ordering::Relaxed);
                let ticks = snapshot.tick.wrapping_sub(last_tick);
                let notifies = snapshot.notifies;
                last_tick = snapshot.tick;
                let mut line = format!(
                    "{} renders={renders}/s ticks={ticks}/s notifies={notifies}/s active={} clock={} leases={}",
                    chrono::Local::now().format("%H:%M:%S"),
                    crate::pulse::app_active(cx),
                    if snapshot.running { "running" } else { "parked" },
                    snapshot.leases.len(),
                );
                for lease in &snapshot.leases {
                    line.push_str(&format!(
                        "\n  {:<14} {:<7} painted={:<5} {}:{}",
                        format!("{:?}", lease.view),
                        format!("{:?}", lease.rate),
                        lease.painted,
                        lease.location.file(),
                        lease.location.line(),
                    ));
                }
                line
            });
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{line}");
            }
        }
    })
    .detach();
}
