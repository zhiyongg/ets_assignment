use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam::channel::unbounded;
mod share;
mod sensor_multiThread;
mod actuator_multiThread;
mod actuatorCommander_multiThread;

use crate::actuatorCommander_multiThread::ActuatorCommander;
use crate::share::{BenchmarkStats, SensorType, SystemLog};
use crate::sensor_multiThread::Sensor;
use crate::actuator_multiThread::Actuator;
// use tokio::time::{self, Duration};

pub fn run_simulation(duration: Duration) {
    println!("--- Starting Real-Time Sensor Simulation ---");

    // 1. Setup Shared Resources

    // CHANNEL: Sensor -> Commander
    let (tx_force, rx_force) = unbounded();
    let (tx_pos, rx_pos) = unbounded();
    let (tx_temp, rx_temp) = unbounded();

    // CHANNEL: Actuator -> Commander
    let (fb_tx_force, fb_rx_force) = unbounded();
    let (fb_tx_pos, fb_rx_pos) = unbounded();
    let (fb_tx_temp, fb_rx_temp) = unbounded();

    // let mut feedback_rx_map = HashMap::new();
    // feedback_rx_map.insert(SensorType::Force, fb_rx_force);
    // feedback_rx_map.insert(SensorType::Position, fb_rx_pos);
    // feedback_rx_map.insert(SensorType::Temperature, fb_rx_temp);

    // CHANNEL: Commander -> Actuator
    let (at_tx_force, at_rx_force) = unbounded();
    let (at_tx_pos, at_rx_pos) = unbounded();
    let (at_tx_temp, at_rx_temp) = unbounded();

    let mut actuator_tx_map = HashMap::new();
    actuator_tx_map.insert(SensorType::Force, at_tx_force);
    actuator_tx_map.insert(SensorType::Position, at_tx_pos);
    actuator_tx_map.insert(SensorType::Temperature, at_tx_temp);

    // CHANNEL: Commander -> Sensor
    // let (fbs_tx_force, fbs_rx_force) = unbounded();
    // let (fbs_tx_pos, fbs_rx_pos) = unbounded();
    // let (fbs_tx_temp, fbs_rx_temp) = unbounded();

    // let mut feedback_tx_map = HashMap::new();
    // feedback_tx_map.insert(SensorType::Force, fbs_tx_force);
    // feedback_tx_map.insert(SensorType::Position, fbs_tx_pos);
    // feedback_tx_map.insert(SensorType::Temperature, fbs_tx_temp);

    // BenchMark Report
    let mut benchmark_stats = BenchmarkStats::new();

    // 2. Define the Sensors we want to run
    let sensors = vec![
        SensorType::Force,
        SensorType::Position,
        SensorType::Temperature,
    ];

    let system_log = Arc::new(Mutex::new(SystemLog::new()));
    let sensor_log = system_log.clone();
    let commander_log = system_log.clone();
    let actuator_log = system_log.clone();

    let sensor_temperature = Sensor::new(SensorType::Temperature,sensor_log.clone());
    let sensor_position = Sensor::new(SensorType::Position,sensor_log.clone());
    let sensor_force = Sensor::new(SensorType::Force,sensor_log.clone());

    let start_time = Instant::now();

    // Spawn sensor threads and CAPTURE handles
    let temp_handle = thread::spawn(move || {
        sensor_temperature.run(tx_temp, fb_rx_temp)
    });

    let pos_handle = thread::spawn(move || {
        sensor_position.run(tx_pos, fb_rx_pos)
    });

    let force_handle = thread::spawn(move || {
        sensor_force.run(tx_force, fb_rx_force)
    });

    let commander = ActuatorCommander::new(actuator_tx_map, commander_log);
    let commander_handle = thread::spawn(move || {
        commander.run(rx_force, rx_pos, rx_temp)
    });

    let mut motor = Actuator::new(
        "Motor".to_string(),
        SensorType::Temperature,
        actuator_log.clone()
    );

    let mut stabiliser = Actuator::new(
        "Stabiliser".to_string(),
        SensorType::Position,
        actuator_log.clone()
    );

    let mut gripper = Actuator::new(
        "Gripper".to_string(),
        SensorType::Force,
        actuator_log.clone()
    );


    let motor_handle = thread::spawn({
        move || {
            motor.run(at_rx_temp, fb_tx_temp)
        }
    });

    let stabiliser_handle = thread::spawn({
        move || {
            stabiliser.run(at_rx_pos, fb_tx_pos)
        }
    });

    let gripper_handle = thread::spawn({
        move || {
            gripper.run(at_rx_force, fb_tx_force)
        }
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

    let total_run_time = start_time.elapsed();

    let temp_stats = temp_handle.join().unwrap_or_else(|_| BenchmarkStats::new());
    let pos_stats = pos_handle.join().unwrap_or_else(|_| BenchmarkStats::new());
    let force_stats = force_handle.join().unwrap_or_else(|_| BenchmarkStats::new());

    let commander_stats = commander_handle.join().unwrap_or_else(|_| BenchmarkStats::new());

    let motor_stats = motor_handle.join().unwrap_or_else(|_| BenchmarkStats::new());
    let stabiliser_stats = stabiliser_handle.join().unwrap_or_else(|_| BenchmarkStats::new());
    let gripper_stats = gripper_handle.join().unwrap_or_else(|_| BenchmarkStats::new());

    benchmark_stats.merge(&temp_stats);
    benchmark_stats.merge(&pos_stats);
    benchmark_stats.merge(&force_stats);
    benchmark_stats.merge(&commander_stats);
    benchmark_stats.merge(&motor_stats);
    benchmark_stats.merge(&stabiliser_stats);
    benchmark_stats.merge(&gripper_stats);

    print_report(benchmark_stats, total_run_time);

}

pub fn print_report(benchmark_stats: BenchmarkStats, total_run_time: Duration){
    println!("\n===== Sensor Summary =====");
    println!("  Total Cycles:      {}", benchmark_stats.sensor_count);
    println!("  Throughput:        {:.2} pkts/sec", benchmark_stats.throughput(total_run_time));
    println!("  Missed Deadlines:  {} ({:.2}%)", benchmark_stats.sensor_missed_deadlines, benchmark_stats.sensor_deadline_rate());
    println!("  Total Generation:  {:?}", benchmark_stats.total_gen_time);
    println!("  Total Processing:  {:?}", benchmark_stats.total_proc_time);
    println!("  Total Transmit:    {:?}", benchmark_stats.total_trans_time);
    println!("  Avg Generation:    {:?}", benchmark_stats.avg_gen());
    println!("  Avg Processing:    {:?}", benchmark_stats.avg_proc());
    println!("  Avg Transmit:      {:?}", benchmark_stats.avg_trans());
    println!("  Avg Jitter:        {:?} (Max: {:?})", benchmark_stats.avg_jitter(), benchmark_stats.max_jitter);

    println!("\n===== Actuator Summary =====");
    println!("  Total Cycles:         {}", benchmark_stats.actuator_count);
    println!("  Throughput:           {:.2} pkts/sec", benchmark_stats.actuator_count as f64 / total_run_time.as_secs_f64());
    println!("  Missed Deadlines:     {} ({:.2}%)", benchmark_stats.actuator_missed_deadlines, benchmark_stats.actuator_deadline_rate());
    println!("  Total Execution Time: {:?}", benchmark_stats.total_actuator_time);
    println!("  Avg Execution Time:   {:?}", benchmark_stats.avg_actuator());
    println!("  Total E2E Latency:    {:?}", benchmark_stats.total_latency);
    println!("  Avg E2E Latency:      {:?}", benchmark_stats.avg_latency());
    println!("  Avg Jitter:           {:?} (Max: {:?})", benchmark_stats.avg_at_jitter(),benchmark_stats.max_at_jitter);

}