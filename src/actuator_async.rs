use tokio::sync::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::time::{self, Duration, Instant};
use rand::Rng;
use crate::share::{BenchmarkStats, Feedback, SensorData, SensorType, SystemLog};

pub struct ActuatorAsync {
    name: String,
    sensor_type: SensorType,
    operation_deadline: Duration,
    log: Arc<Mutex<SystemLog>>,
    benchmark_stats: BenchmarkStats,
    last_arrival_time: Option<std::time::Instant>,
}

impl ActuatorAsync {
    pub fn new(name: String, sensor_type: SensorType, log: Arc<Mutex<SystemLog>>) -> Self {
        Self {
            name,
            sensor_type,
            operation_deadline: Duration::from_micros(2000),
            log,
            benchmark_stats: BenchmarkStats::new(),
            last_arrival_time:None
        }
    }

    fn generate_feedback(&self) -> Feedback {
        let mut rng = rand::rng();
        if rng.random_bool(0.95) {
            Feedback {
                is_ack: true,
                error_msg: "no".to_string(),
                recalibrate_offset: 0.0,
                timestamp: std::time::Instant::now(),
            }
        } else {
            Feedback {
                is_ack: false,
                error_msg: "Drift Check".to_string(),
                recalibrate_offset: rng.random_range(-0.5..0.5),
                timestamp: std::time::Instant::now(),
            }
        }
    }

    fn update_jitter(&mut self) {

        let current_time = std::time::Instant::now();

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

    pub async fn run(
        mut self,
        mut rx_data: Receiver<SensorData>,
        tx_feedback: Sender<Feedback>,
    ) -> BenchmarkStats {

        while let Some(data) = rx_data.recv().await {

            self.update_jitter();

            let start = Instant::now();

            // 1. Simulate Actuation (NON-BLOCKING SLEEP)
            // println!("Actuator [{}] acting...", self.name);
            time::sleep(Duration::from_micros(100)).await;

            // 2. Deadline Check
            if start.elapsed() > self.operation_deadline {
                self.benchmark_stats.actuator_missed_deadlines += 1;
            }

            // 3. Feedback
            let fb = self.generate_feedback();
            if fb.recalibrate_offset != 0.0 {
                let _ = tx_feedback.send(fb).await;
            }

            let duration = start.elapsed();
            self.benchmark_stats.total_actuator_time += duration;

            self.benchmark_stats.actuator_count += 1;

            // E2E Latency calculation
            let now = std::time::Instant::now();
            self.benchmark_stats.total_latency += now.duration_since(data.timestamp);
        }

        self.benchmark_stats
    }
}