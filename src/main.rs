
use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use rts_assignment::share::{BenchmarkStats, SensorType, SystemLog};
use rts_assignment::sensor_async::SensorAsync;
use rts_assignment::actuator_async::ActuatorAsync;
use rts_assignment::actuator_commander_async::ActuatorCommanderAsync;
use rts_assignment::print_report;

// fn main() {
//
//     // Run for the full 2 seconds required for the demo
//     run_simulation(Duration::from_secs(2));
//
// }




//===============================Tokio Main===========================
#[tokio::main]
async fn main() {
    println!("=== Starting Async Real-Time Simulation (Tokio) ===");

    // 1. Setup Shared Logging
    let system_log = Arc::new(Mutex::new(SystemLog::new()));
    let mut benchmark_stats = BenchmarkStats::new();

    // 2. Create Channels (Buffer size 32 is sufficient for real-time)
    // SENSORS -> COMMANDER
    let (tx_force, rx_force) = mpsc::channel(32);
    let (tx_pos, rx_pos) = mpsc::channel(32);
    let (tx_temp, rx_temp) = mpsc::channel(32);

    // COMMANDER -> ACTUATORS
    let (at_tx_force, at_rx_force) = mpsc::channel(32);
    let (at_tx_pos, at_rx_pos) = mpsc::channel(32);
    let (at_tx_temp, at_rx_temp) = mpsc::channel(32);

    // ACTUATORS -> SENSORS (Feedback Loop)
    let (fb_tx_force, fb_rx_force) = mpsc::channel(32);
    let (fb_tx_pos, fb_rx_pos) = mpsc::channel(32);
    let (fb_tx_temp, fb_rx_temp) = mpsc::channel(32);

    // Map needed for Commander initialization
    let mut actuator_tx_map = HashMap::new();
    actuator_tx_map.insert(SensorType::Force, at_tx_force);
    actuator_tx_map.insert(SensorType::Position, at_tx_pos);
    actuator_tx_map.insert(SensorType::Temperature, at_tx_temp);

    // 3. Initialize Components
    let sensor_force = SensorAsync::new(SensorType::Force, system_log.clone());
    let sensor_pos = SensorAsync::new(SensorType::Position, system_log.clone());
    let sensor_temp = SensorAsync::new(SensorType::Temperature, system_log.clone());

    let commander = ActuatorCommanderAsync::new(actuator_tx_map, system_log.clone());

    let actuator_force = ActuatorAsync::new("Gripper".to_string(), SensorType::Force, system_log.clone());
    let actuator_pos = ActuatorAsync::new("Stabiliser".to_string(), SensorType::Position, system_log.clone());
    let actuator_temp = ActuatorAsync::new("Cooling".to_string(), SensorType::Temperature, system_log.clone());

    let start_time = std::time::Instant::now();

    // 4. Spawn Async Tasks
    // We use join handles to collect the stats at the end
    let h_s_force = tokio::spawn(async move { sensor_force.run(tx_force, fb_rx_force).await });
    let h_s_pos = tokio::spawn(async move { sensor_pos.run(tx_pos, fb_rx_pos).await });
    let h_s_temp = tokio::spawn(async move { sensor_temp.run(tx_temp, fb_rx_temp).await });

    let mut h_commander = tokio::spawn(async move { commander.run(rx_force, rx_pos, rx_temp).await });

    let h_a_force = tokio::spawn(async move { actuator_force.run(at_rx_force, fb_tx_force).await });
    let h_a_pos = tokio::spawn(async move { actuator_pos.run(at_rx_pos, fb_tx_pos).await });
    let h_a_temp = tokio::spawn(async move { actuator_temp.run(at_rx_temp, fb_tx_temp).await });

    // 5. Run Simulation Duration
    println!("Simulation Running...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    {
        let mut log = system_log.lock().await;
        log.active = false; // This tells everyone to break their loops
        log.write("--- Stopping Simulation ---".to_string());
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let total_run_time = start_time.elapsed();

    let temp_stats = h_s_force.await.unwrap();
    let pos_stats = h_s_pos.await.unwrap();
    let force_stats = h_s_temp.await.unwrap();

    let commander_stats = h_commander.await.unwrap();
    let motor_stats = h_a_force.await.unwrap();
    let stabiliser_stats = h_a_pos.await.unwrap();
    let gripper_stats = h_a_temp.await.unwrap();

    benchmark_stats.merge(&temp_stats);
    benchmark_stats.merge(&pos_stats);
    benchmark_stats.merge(&force_stats);
    benchmark_stats.merge(&commander_stats);
    benchmark_stats.merge(&motor_stats);
    benchmark_stats.merge(&stabiliser_stats);
    benchmark_stats.merge(&gripper_stats);

    println!("--- Simulation Finished (Async) ---");
    // (In a real implementation, you would signal cancellation here to let threads return)
    print_report(benchmark_stats, total_run_time);
}

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
