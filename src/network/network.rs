use crate::settings::settings::Settings;

use eframe::egui::{self, Spinner};
use std::{
    collections::HashSet,
    process::Command,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
};

pub struct Network {
    devices: Vec<Device>,
    live_wifis: Vec<Wifi>,
    known_wifis: HashSet<String>,
    current_wifi_id: u8,
    scanwifiing: Arc<Mutex<bool>>,
    tx: Sender<Vec<Wifi>>,
    rx: Receiver<Vec<Wifi>>,
    init: bool,
}

#[derive(Default)]
struct Device {
    device: String,
    status: bool,
}

#[derive(Default)]
struct Wifi {
    id: u8,
    bssid: String,
    ssid: String,
    mode: String,
    chan: u8,
    rate: String,
    signal: u8,
    bras: String,
    security: String,
}

impl Default for Network {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            devices: Vec::new(),
            live_wifis: Vec::new(),
            known_wifis: HashSet::new(),
            current_wifi_id: 0,
            scanwifiing: Arc::new(Mutex::new(false)),
            init: false,
            tx,
            rx,
        }
    }
}

impl Settings for Network {
    fn init(&mut self) {
        // 1. get devices
        self.get_devices();

        // 2. scan wifi and get wifi info
        self.scan_wifi();

        // 3. get known_wifis
        self.get_known_wifi();

        // 4. change init status
        self.init = true;
    }

    fn is_init(&self) -> bool {
        self.init
    }

    fn name(&self) -> &str {
        "Network"
    }

    fn apply(&mut self) {
        // 1. apply devices
        for device in self.devices.iter() {
            let mut connect = "disconnect";
            if device.status {
                connect = "connect";
            }
            Command::new("nmcli")
                .args(["device", connect, device.device.as_str()])
                .spawn()
                .unwrap();
        }
        // 2. apply wifi
        for wifi in self.live_wifis.iter() {
            if self.current_wifi_id == wifi.id {
                Command::new("nmcli")
                    .args(["dev", "wifi", "connect", wifi.ssid.as_str()])
                    .spawn()
                    .unwrap();
            }
        }
    }

    fn show(&mut self, ui: &mut eframe::egui::Ui) {
        egui::Grid::new("network grid")
            .num_columns(3)
            .show(ui, |ui| {
                for device in self.devices.iter_mut() {
                    device.show(ui);
                }
            });
        ui.separator();
        egui::ScrollArea::both().show(ui, |ui| {
            if let Ok(scanwifiing) = self.scanwifiing.try_lock() {
                if *scanwifiing {
                    ui.add(Spinner::new());
                } else {
                    drop(scanwifiing);
                    if ui.button("Scan Wifi").clicked() {
                        self.scan_wifi();
                    }
                    ui.end_row();
                    if let Ok(wifis) = self.rx.try_recv() {
                        self.live_wifis = wifis;
                    }
                    egui::Grid::new("wifi").num_columns(6).show(ui, |ui| {
                        for wifi in self.live_wifis.iter_mut() {
                            wifi.show(ui, &mut self.current_wifi_id);
                            ui.end_row();
                        }
                    });
                }
            } else {
                ui.add(Spinner::new());
            }
        });
    }
}

impl Network {
    fn get_devices(&mut self) {
        let output = Command::new("nmcli")
            .args(["-t", "d"])
            .output()
            .expect("execute nmcli error");
        let output = String::from_utf8(output.stdout).unwrap();
        let mut outs = output.lines();
        while let Some(line) = outs.next() {
            let mut datas = line.split(':');
            let mut device = Device::default();
            device.device = datas.next().unwrap().to_string();
            device.status = true;
            self.devices.push(device);
        }
    }

    fn scan_wifi(&self) {
        let mut scanwifiing = self.scanwifiing.lock().unwrap();
        *scanwifiing = true;
        drop(scanwifiing);
        let scanwifiing = self.scanwifiing.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            // 1. scan wifi
            println!("scan wifi");
            let output = Command::new("nmcli")
                .args(["-t", "device", "wifi", "list"])
                .output()
                .unwrap();
            let stdout = String::from_utf8(output.stdout).unwrap();
            let mut lines = stdout.lines();

            // 2. parser wifi
            let mut id = 0;
            let mut wifis = Vec::new();
            while let Some(line) = lines.next() {
                let mut wifi = Wifi::default();
                // a. need to deal with bssid separately
                let line = line.replace("\\:", "-");
                let mut data = line.split(':');
                let current = data.next().unwrap();
                wifi.bssid = data.next().unwrap().replace('-', ":");
                wifi.ssid = data.next().unwrap().to_string();
                if wifi.ssid.is_empty() {
                    continue;
                }
                wifi.mode = data.next().unwrap().to_string();
                wifi.chan = data.next().unwrap().parse().unwrap();
                wifi.rate = data.next().unwrap().to_string();
                wifi.signal = data.next().unwrap().parse().unwrap();
                wifi.bras = data.next().unwrap().to_string();
                wifi.security = data.next().unwrap().to_string();
                wifi.id = id;

                // b. if current wifi
                if current == "*" {
                    wifi.ssid.push_str(" (*)");
                }
                wifis.push(wifi);
                id += 1;
            }
            let mut scanwifiing = scanwifiing.lock().unwrap();
            *scanwifiing = false;
            drop(scanwifiing);
            tx.send(wifis).unwrap();
            println!("end scan wifi");
        });
    }

    fn get_known_wifi(&mut self) {
        let output = Command::new("nmcli")
            .args(["-t", "connection", "show"])
            .output()
            .expect("nmcli execute error");
        let output = String::from_utf8(output.stdout).unwrap();
        let mut lines = output.lines();
        while let Some(line) = lines.next() {
            let mut data = line.split(";");
            self.known_wifis.insert(data.next().unwrap().to_string());
        }
    }
}

impl Device {
    fn show(&mut self, ui: &mut eframe::egui::Ui) {
        ui.label("Device");
        ui.label(self.device.as_str());
        ui.checkbox(&mut self.status, "enable");
        ui.end_row();
    }
}

impl Wifi {
    fn show(&mut self, ui: &mut eframe::egui::Ui, id: &mut u8) {
        ui.radio_value(id, self.id, "");
        ui.label(&self.ssid);
        ui.label(&self.mode);
        ui.label(self.chan.to_string().as_str());
        ui.label(&self.rate);
        ui.label(self.signal.to_string().as_str());
        ui.label(&self.security);
    }
}
