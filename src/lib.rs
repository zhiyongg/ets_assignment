use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam::channel::unbounded;
mod share;
mod sensor_multiThread;
mod actuator_multiThread;

use crate::actuator_multiThread::ActuatorCommander;
use crate::share::{SensorType, SystemLog};
use crate::sensor_multiThread::Sensor;
// use tokio::time::{self, Duration};

pub fn run_simulation(duration: Duration) {
    println!("--- Starting Real-Time Sensor Simulation ---");

    // 1. Setup Shared Resources
    let (tx_force, rx_force) = unbounded();
    let (tx_pos, rx_pos) = unbounded();
    let (tx_temp, rx_temp) = unbounded();

    let (fb_tx_force, fb_rx_force) = unbounded();
    let (fb_tx_pos, fb_rx_pos) = unbounded();
    let (fb_tx_temp, fb_rx_temp) = unbounded();

    let mut feedback_map = HashMap::new();
    feedback_map.insert(SensorType::Force, fb_tx_force);
    feedback_map.insert(SensorType::Position, fb_tx_pos);
    feedback_map.insert(SensorType::Temperature, fb_tx_temp);

    // 2. Define the Sensors we want to run
    let sensors = vec![
        SensorType::Force,
        SensorType::Position,
        SensorType::Temperature,
    ];

    let system_log = Arc::new(Mutex::new(SystemLog::new()));

    // 3. Spawn Threads for each Sensor
    let spawn_sensor = |sType, tx, rx_fb, log| {
        thread::spawn(move || {
            let sensor = Sensor::new(sType);
            // Sensor::run now takes rx_fb
            sensor.run(tx, rx_fb, log);
        });
    };

    spawn_sensor(SensorType::Force, tx_force, fb_rx_force, system_log.clone());
    spawn_sensor(SensorType::Position, tx_pos, fb_rx_pos, system_log.clone());
    spawn_sensor(SensorType::Temperature, tx_temp, fb_rx_temp, system_log.clone());

    // 4. Run a Dummy Receiver (Actuator Placeholder)
    // 4. Run Commander (Main Thread blocks here)
    let commander_log = system_log.clone();
    let commander = ActuatorCommander::new(feedback_map, commander_log);

    let commander_handle = thread::spawn(move || {
        commander.run(rx_force, rx_pos, rx_temp);
    });

    thread::sleep(duration);

    {
        // Use the copy of system_log kept by main to signal shutdown
        if let Ok(mut log) = system_log.lock() {
            log.active = false; // This tells sensors to break their loop
        }
    }

    // Optional: Wait a tiny bit for threads to see the flag and clean up
    thread::sleep(Duration::from_millis(1000));

    println!("--- Simulation Finished ---");
}