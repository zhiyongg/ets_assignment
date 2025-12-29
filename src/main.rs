
mod share;
mod sensor_tokio;
mod actuator3;
mod sensor_multiThread;
mod actuator_multiThread;

use std::collections::HashMap;
// use tokio::sync::{mpsc, broadcast, Mutex};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossbeam::channel::unbounded;
use rts_assignment::run_simulation;
use crate::actuator_multiThread::ActuatorCommander;
use crate::share::{SensorType, SystemLog};
use crate::sensor_multiThread::Sensor;
// use tokio::time::{self, Duration};


fn main() {

    // Run for the full 2 seconds required for the demo
    run_simulation(Duration::from_secs(2));

}




//===============================Tokio Main===========================
// #[tokio::main]
// async fn main() {
//     println!("=== System Start ===");
//     let log = Arc::new(Mutex::new(SystemLog::new()));
//
//     // 1. Create THREE distinct channels (one for each sensor type)
//     let (tx_force, rx_force) = mpsc::channel(100);
//     let (tx_pos, rx_pos) = mpsc::channel(100);
//     let (tx_temp, rx_temp) = mpsc::channel(100);
//
//     // 2. Feedback Channel (Broadcast)
//     let (tx_feedback, _) = broadcast::channel(100);
//
//     // 3. Spawn Sensors (Each gets its own specific TX channel)
//
//     // --- Sensor 1: Force ---
//     let s1_fb = tx_feedback.subscribe();
//     let s1_log = log.clone();
//     tokio::spawn(async move {
//         run_sensor(SensorType::Force, tx_force, s1_fb, s1_log).await;
//     });
//
//     // --- Sensor 2: Position ---
//     let s2_fb = tx_feedback.subscribe();
//     let s2_log = log.clone();
//     tokio::spawn(async move {
//         run_sensor(SensorType::Position, tx_pos, s2_fb, s2_log).await;
//     });
//
//     // --- Sensor 3: Temperature ---
//     let s3_fb = tx_feedback.subscribe();
//     let s3_log = log.clone();
//     tokio::spawn(async move {
//         run_sensor(SensorType::Temperature, tx_temp, s3_fb, s3_log).await;
//     });
//
//     // 4. Spawn Actuator Manager
//     // Pass the 3 distinct receivers to the manager
//     let act_log = log.clone();
//     tokio::spawn(async move {
//         run_actuator(rx_force, rx_pos, rx_temp, tx_feedback, act_log).await;
//     });
//
//     // 5. Run Simulation
//     time::sleep(Duration::from_secs(2)).await;
//     println!("=== Test End ===");
//
//     // 6. Dump Logs
//     let log_guard = log.lock().await;
//     for entry in &log_guard.entries {
//         println!("LOG: {}", entry);
//     }
// }



// fn main(){
// println!("--- Starting Real-Time Sensor Simulation ---");
//
// // 1. Setup Shared Resources
// let (tx_force, rx_force) = unbounded();
// let (tx_pos, rx_pos) = unbounded();
// let (tx_temp, rx_temp) = unbounded();
//
// let (fb_tx_force, fb_rx_force) = unbounded();
// let (fb_tx_pos, fb_rx_pos) = unbounded();
// let (fb_tx_temp, fb_rx_temp) = unbounded();
//
// let mut feedback_map = HashMap::new();
// feedback_map.insert(SensorType::Force, fb_tx_force);
// feedback_map.insert(SensorType::Position, fb_tx_pos);
// feedback_map.insert(SensorType::Temperature, fb_tx_temp);
//
// // 2. Define the Sensors we want to run
// let sensors = vec![
//     SensorType::Force,
//     SensorType::Position,
//     SensorType::Temperature,
// ];
//
// let system_log = Arc::new(Mutex::new(SystemLog::new()));
//
// // 3. Spawn Threads for each Sensor
// let spawn_sensor = |sType, tx, rx_fb, log| {
// thread::spawn(move || {
// let sensor = Sensor::new(sType);
// // Sensor::run now takes rx_fb
// sensor.run(tx, rx_fb, log);
// });
// };
//
// spawn_sensor(SensorType::Force, tx_force, fb_rx_force, system_log.clone());
// spawn_sensor(SensorType::Position, tx_pos, fb_rx_pos, system_log.clone());
// spawn_sensor(SensorType::Temperature, tx_temp, fb_rx_temp, system_log.clone());
//
// // 4. Run a Dummy Receiver (Actuator Placeholder)
// // 4. Run Commander (Main Thread blocks here)
// let commander_log = system_log.clone();
// let commander = ActuatorCommander::new(feedback_map, commander_log);
//
// let commander_handle = thread::spawn(move || {
// commander.run(rx_force, rx_pos, rx_temp);
// });
//
// thread::sleep(Duration::from_secs(2));
//
// {
// // Use the copy of system_log kept by main to signal shutdown
// if let Ok(mut log) = system_log.lock() {
// log.active = false; // This tells sensors to break their loop
// }
// }
//
// // Optional: Wait a tiny bit for threads to see the flag and clean up
// thread::sleep(Duration::from_millis(1000));
//
// println!("--- Simulation Finished ---");
// }