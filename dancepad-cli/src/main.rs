mod serial;

use std::{
    iter,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use abi::AdcValues;
use env_logger::Env;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::error;
use parking_lot::{Condvar, Mutex};

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let running = Arc::new(AtomicBool::new(true));
    let adc_values = Arc::new(RwLock::new(AdcValues::<4>::default()));
    let pair = Arc::new((Mutex::new(false), Condvar::new()));

    let m = MultiProgress::new();
    let style = ("ADC ", "█▉▊▋▌▍▎▏  ", "green");
    let handles: Vec<_> = iter::repeat(style)
        .enumerate()
        .take(4)
        .map(|(adc_idx, s)| {
            let pb = m.add(ProgressBar::new(1000));
            pb.set_style(
                // TODO: if above theshold, change color to red (use .set_style())
                ProgressStyle::with_template(&format!(
                    "{{prefix:.bold}} {}▕{{bar:.{}}}▏{{msg}}",
                    adc_idx, s.2
                ))
                .unwrap()
                .progress_chars(s.1),
            );
            pb.set_prefix(s.0);

            let running = Arc::clone(&running);
            let data = Arc::clone(&adc_values);
            {
                let pair = pair.clone();
                thread::spawn(move || {
                    let &(ref lock, ref cvar) = &*pair;
                    let mut started = lock.lock();
                    while running.load(Ordering::Acquire) {
                        cvar.wait(&mut started);
                        let adc = data.read().unwrap()[adc_idx];
                        pb.set_position(adc as u64);
                        pb.set_message(format!("{:3} %", adc / 10));
                    }
                })
            }
        })
        .collect();

    let mut port = serial::open().unwrap();

    // indicatif can draw max 20 times per sec (== 50 ms)
    const RQ_PERIOD: Duration = Duration::from_millis(50);

    thread::spawn(move || {
        let (lock, cvar) = &*pair;
        while running.load(Ordering::Acquire) {
            let cmd = abi::Command::GetValues;
            let response = serial::exchange(&cmd, &mut port).unwrap();
            match response {
                abi::Response::Values4(values) => {
                    *adc_values.write().unwrap() = values.into();
                    let mut started = lock.lock();
                    *started = true;
                    cvar.notify_all();
                }
                _ => {
                    error!(
                        "received incorrect response to command: {:?} -> {:?}",
                        cmd, response
                    )
                }
            }
            thread::sleep(RQ_PERIOD);
        }
    });

    for h in handles {
        let _ = h.join();
    }
}
