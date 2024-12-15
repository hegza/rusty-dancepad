mod serial;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use abi::{AdcValues, Response};
use eframe::egui::{self, Color32, Slider, TextStyle, mutex::Mutex};
use egui_extras::{Size, StripBuilder};
use env_logger::Env;
use log::{error, info};
use parking_lot::RwLock;
use serial2::SerialPort;

/// Shows off a table with dynamic layout
pub struct App {
    adc_vals: Arc<RwLock<AdcValues<4>>>,
    thresh_vals: Arc<RwLock<AdcValues<4>>>,
    port: Arc<Mutex<SerialPort>>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();
        self.show(&ctx);
    }
}
impl App {
    fn show(&mut self, ctx: &egui::Context) {
        egui::SidePanel::new(egui::panel::Side::Left, "Control").show(ctx, |ui| {
            if ui.button("Commit").clicked() {
                let th = self.thresh_vals.read().0;
                let cmd = abi::Command::SetThresh4(th);
                let resp = serial::exchange(&cmd, &mut self.port.lock()).unwrap();
                if resp != Response::Ok {
                    error!(
                        "received incorrect response to command: {:?} -> {:?}",
                        cmd, resp
                    )
                } else {
                    info!("Set thresholds {th:?}");
                }
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.ui(ui);
        });
    }
}

impl App {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let dark_mode = ui.visuals().dark_mode;
        let faded_color = ui.visuals().window_fill();
        let faded_color = |color: Color32| -> Color32 {
            use egui::Rgba;
            let t = if dark_mode { 0.95 } else { 0.8 };
            egui::lerp(Rgba::from(color)..=Rgba::from(faded_color), t).into()
        };

        let adc_header_text_size = TextStyle::Body.resolve(ui.style()).size;
        StripBuilder::new(ui)
            .size(Size::relative(0.05))
            .size(Size::relative(0.15))
            .size(Size::relative(0.05))
            .size(Size::relative(0.15))
            .size(Size::relative(0.05))
            .size(Size::relative(0.15))
            .size(Size::relative(0.05))
            .size(Size::relative(0.15))
            .horizontal(|mut adc_bars| {
                for adc_idx in 0..4 {
                    let adc_value = self.adc_vals.read().0[adc_idx];
                    const ADC_MAX_VALUE: f32 = 1000.;
                    let adc_percent = adc_value as f32 / ADC_MAX_VALUE;
                    adc_bars.cell(|ui| {
                        ui.allocate_space(egui::Vec2 {
                            x: 0.,
                            y: adc_header_text_size,
                        });
                        ui.add_sized(
                            egui::Vec2 {
                                x: ui.available_width(),
                                y: ui.available_height(),
                            },
                            Slider::new(
                                &mut self.thresh_vals.write().0[adc_idx],
                                ADC_MAX_VALUE as u16..=0,
                            )
                            .vertical(),
                        );
                    });
                    adc_bars.strip(|builder| {
                        builder
                            // Text
                            .size(Size::exact(adc_header_text_size))
                            // ADC value
                            .size(Size::relative(adc_percent))
                            // Remainder for the max
                            .size(Size::remainder())
                            .vertical(|mut strip| {
                                strip.cell(|ui| {
                                    ui.label(format!("ADC {adc_idx}"));
                                });
                                // Top cell: the value
                                strip.cell(|ui| {
                                    ui.painter().rect_filled(
                                        ui.available_rect_before_wrap(),
                                        0.0,
                                        faded_color(
                                            if self.adc_vals.read().0[adc_idx]
                                                >= self.thresh_vals.read().0[adc_idx]
                                            {
                                                Color32::LIGHT_GREEN
                                            } else {
                                                Color32::YELLOW
                                            },
                                        ),
                                    );
                                });
                                // Bottom cell: remaining until max value
                                strip.cell(|ui| {
                                    ui.painter().rect_filled(
                                        ui.available_rect_before_wrap(),
                                        0.0,
                                        faded_color(Color32::BLUE),
                                    );
                                });
                            });
                    });
                    /*
                    ui.put(
                        Rect::from_min_max(bar_pos, bar_pos + Vec2::new(100., 0.)),
                        egui::Shape::line_segment(
                            [Pos2::new(0., 0.), Pos2::new(100., 0.)],
                            PathStroke::default(),
                        ),
                    );*/
                }
            });
    }
}

fn spawn_adc_updater(
    port: Arc<Mutex<SerialPort>>,
    adc_values: Arc<RwLock<AdcValues<4>>>,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    const RQ_PERIOD: Duration = Duration::from_millis(15);
    thread::spawn(move || {
        while running.load(Ordering::Acquire) {
            let cmd = abi::Command::GetValues;
            let response = serial::exchange(&cmd, &mut port.lock()).unwrap();
            match response {
                abi::Response::Values4(values) => {
                    *adc_values.write() = values.into();
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
    })
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let port = serial::open().unwrap();
    port.discard_buffers().unwrap();
    port.flush().unwrap();
    let port = Arc::new(Mutex::new(port));
    let mut thresh = AdcValues::<4>::default();
    let cmd = abi::Command::GetThresh;
    match serial::exchange(&cmd, &mut port.lock()).unwrap() {
        abi::Response::Values4(th) => {
            thresh = th.into();
            info!("Set thresholds {thresh:?}");
        }
        resp => error!(
            "received incorrect response to command: {:?} -> {:?}",
            cmd, resp
        ),
    };
    let thresh_vals = Arc::new(RwLock::new(thresh));
    let adc_vals = Arc::new(RwLock::new(AdcValues::<4>::default()));

    let running = Arc::new(AtomicBool::new(true));
    let handle = spawn_adc_updater(
        Arc::clone(&port),
        Arc::clone(&adc_vals),
        Arc::clone(&running),
    );
    thread::sleep(Duration::from_millis(200));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|_cc| {
            Ok(Box::new(App {
                adc_vals,
                thresh_vals,
                port,
            }))
        }),
    )?;

    // Make sure all exchanges are fully completed before exiting
    running.store(false, Ordering::Release);
    handle.join().unwrap();

    return eframe::Result::Ok(());
}
