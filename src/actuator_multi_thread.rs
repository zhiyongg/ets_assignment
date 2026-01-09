use std::ops::Deref;
use std::sync::{Arc, Mutex};
use crossbeam::channel::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use rand::Rng;
use crate::share::{BenchmarkStats, Feedback, SensorData,SensorType, SystemLog};

pub struct Actuator{
    name: String,
    sensor_type: SensorType,
    operation_deadline:Duration,
    log:Arc<Mutex<SystemLog>>,
    benchmark_stats: BenchmarkStats,
    last_arrival_time: Option<Instant>,
}

impl Actuator{
    pub fn new(name: String, sensor_type: SensorType, log: Arc<Mutex<SystemLog>>,) -> Self {

        // 1. Determine the operation deadline
        let deadline = match sensor_type {
            SensorType::Force => Duration::from_micros(2000),
            SensorType::Position => Duration::from_micros(2000),
            SensorType::Temperature => Duration::from_micros(2000),
        };

        Self{name, sensor_type, operation_deadline: deadline, log, benchmark_stats: BenchmarkStats::new(), last_arrival_time:None}
    }

    fn update_jitter(&mut self) {

        let current_time = Instant::now();

        // 1. Check if we have a previous packet time to compare against
        if let Some(last_time) = self.last_arrival_time{
            let interval = current_time.duration_since(last_time);

            // Define expected rate (Consider making this a constant or struct field later)
            let expected_interval = Duration::from_millis(5);

            // Calculate absolute difference (Jitter)
            let jitter = if interval > expected_interval {
                interval - expected_interval
            } else {
                expected_interval - interval
            };

            // Update Stats
            self.benchmark_stats.total_at_jitter += jitter;
            if jitter > self.benchmark_stats.max_at_jitter {
                self.benchmark_stats.max_at_jitter = jitter;
            }
        }
        self.last_arrival_time = Some(current_time);
    }

    fn generate_feedback(&self) -> Feedback {
        let mut rng = rand::rng();

        // Roll dice (0.0 to 1.0)
        let normal: bool = rng.random_bool(0.95);

        // 95% Chance: Everything is fine (Ack)
        if normal {
            Feedback {
                is_ack: true,
                error_msg: "no".to_string(),
                recalibrate_offset: 0.0,
                timestamp: Instant::now(),
            }
        } else {
            // 5% Chance: Randomly request a sensor adjustment
            let random_offset = rng.random_range(-0.5..0.5);

            // Log this command so you can trace it in the report
            if let Ok(mut guard) = self.log.lock() {
                guard.write(format!("[Command] Actuator [{}] req offset: {:.4}", self.name, random_offset));
            }

            Feedback {
                is_ack: false,
                error_msg: "Random Drift Check".to_string(),
                recalibrate_offset: random_offset,
                timestamp: Instant::now(),
            }
        }
    }

    pub fn run(&mut self, sensor_data:Receiver<SensorData>,tx_status: Sender<Feedback>,
    )-> BenchmarkStats{

        // 1. Receive Value from commander
        while let Ok(data) = sensor_data.recv() {

            self.update_jitter();

            // 2. Start to record processing time
            let start = Instant::now();

            // 3. Simulate Actuation
            println!("Actuator [{}] adjusting to effort {:.2}", self.name, data.value);
            thread::sleep(Duration::from_micros(100));

            // 4. Check deadline for the
            let operation_duration = start.elapsed();
            if operation_duration > self.operation_deadline {
                if let Ok(mut log_guard) = self.log.lock() {
                    log_guard.write(format!("[Deadline] Actuator [{}] missed deadline by {:?} ms", self.name, operation_duration-self.operation_deadline));
                }
                self.benchmark_stats.actuator_missed_deadlines += 1;
            }

            // 5. Generate feedback
            let feedback = self.generate_feedback();

            // 6. Send feedback
            if feedback.recalibrate_offset != 0.0 {
                let _ = tx_status.send(feedback);
            }
            
            let duration = start.elapsed();
            let now = Instant::now();
            self.benchmark_stats.total_actuator_time += duration;
            let e2e_latency = now.duration_since(data.timestamp);

            self.benchmark_stats.total_latency += e2e_latency;
            self.benchmark_stats.actuator_count += 1;
        }
        self.benchmark_stats
    }


}