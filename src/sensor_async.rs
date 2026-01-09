use tokio::sync::mpsc::{Sender, Receiver};
use std::collections::VecDeque;
use tokio::sync::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{self, Instant};
use rand::Rng;
use crate::share::{BenchmarkStats, Feedback, SensorData, SensorType, SystemLog};

pub struct SensorAsync {
    id_counter: i32,
    history_buffer: VecDeque<f64>,
    sensor_type: SensorType,
    calibration_offset: f64,
    log: Arc<Mutex<SystemLog>>,
    benchmark_stats: BenchmarkStats,
}

impl SensorAsync {
    pub fn new(sensor_type: SensorType, log: Arc<Mutex<SystemLog>>) -> Self {
        Self {
            id_counter: 0,
            history_buffer: VecDeque::new(),
            sensor_type,
            calibration_offset: 0.0,
            log,
            benchmark_stats: BenchmarkStats::new(),
        }
    }

    fn generate_data(&mut self) -> SensorData {
        let mut rng = rand::rng(); // rand::rng() is thread-local, safe in async tasks
        self.id_counter += 1;

        let value = match self.sensor_type {
            SensorType::Force => rng.random_range(10.0..55.0),
            SensorType::Position => rng.random_range(-0.1..0.2),
            SensorType::Temperature => rng.random_range(20.0..130.0),
        };

        SensorData {
            id: self.id_counter,
            sensor_type: self.sensor_type,
            value: value + self.calibration_offset,
            anomaly: false,
            timestamp: std::time::Instant::now(),
            processed_timestamp: None,
        }
    }

    async fn process_data(&mut self, mut data: SensorData) -> Option<SensorData> {
        let start = Instant::now();

        // 1. Detect Anomaly
        match data.sensor_type {
            SensorType::Force => if data.value < 5.0 || data.value > 60.0 { data.anomaly = true; },
            SensorType::Position => if data.value.abs() > 0.5 { data.anomaly = true; },
            SensorType::Temperature => if data.value > 120.0 { data.anomaly = true; },
        }

        if data.anomaly { return Some(data); }

        // 2. Moving Average
        if self.history_buffer.len() >= 5 { self.history_buffer.pop_front(); }
        self.history_buffer.push_back(data.value);
        let total: f64 = self.history_buffer.iter().sum();
        data.value = total / self.history_buffer.len() as f64;

        data.processed_timestamp = Some(std::time::Instant::now());

        // 3. Deadline Check
        if start.elapsed() > Duration::from_micros(200) {
            self.benchmark_stats.sensor_missed_deadlines += 1;
            let mut log = self.log.lock().await;
            log.write(format!("[Sensor {:?}] Processing Deadline Missed! Dropping.", data.sensor_type));
            return None;
        }

        Some(data)
    }

    async fn transmit_data(&self, sender: &Sender<SensorData>, data: SensorData) -> bool {

        let fault_roll: f64 = {
            let mut rng = rand::rng();
            rng.random_range(0.00..1.00)
        };

        // FAULT 1: Packet Drop (5% chance)
        if fault_roll < 0.05 {
            let start_lock = Instant::now();

            // ASYNC MUTEX LOCK: No "if let Ok", just .await
            let mut guard = self.log.lock().await;

            let contention = start_lock.elapsed();
            guard.write(format!("[FAULT] Dropping packet ID {} for {:?} (Lock Wait: {:?})",
                                data.id, self.sensor_type, contention));

            // Return true because we "successfully" handled the logic (by dropping it intentionally)
            return true;
        }

        // FAULT 2: Network Latency Delay (5% chance)
        if fault_roll > 0.95 {
            // IMPORTANT: Use tokio::time::sleep, NOT std::thread::sleep
            time::sleep(Duration::from_micros(30)).await;
        }

        // 2. Transmit data (Async)
        match sender.send(data).await {
            Ok(_) => true,
            Err(_) => {
                println!("[Sensor {:?}] Receiver disconnected. Stopping.", self.sensor_type);
                false
            }
        }
    }

    pub async fn run(
        mut self,
        tx: Sender<SensorData>,
        mut rx_feedback: Receiver<Feedback>,
    ) -> BenchmarkStats {
        // Fixed interval ticker (Alternative to thread::sleep)
        let mut interval = time::interval(Duration::from_millis(5));

        // Define simulation end condition (optional, or rely on channel close)
        let mut active = true;

        let cycle_time = Duration::from_millis(5);
        let mut next_deadline = Instant::now();

        loop {
            {
                let log = self.log.lock().await;
                if !log.active { break; }
            }

            tokio::select! {
                // EVENT 1: Time to generate data (The Ticker)
                _ = interval.tick() => {
                                // A. Capture current time
                    let now = Instant::now();

                    // B. Calculate Jitter (Difference between NOW and EXPECTED)
                    if now > next_deadline {
                        let jitter = now - next_deadline;

                        // C. Update Stats
                        self.benchmark_stats.total_jitter += jitter;
                        if jitter > self.benchmark_stats.max_jitter {
                            self.benchmark_stats.max_jitter = jitter;
                        }
                    }
                    // D. Advance the deadline for the NEXT loop (Fixed 5ms steps)
                    next_deadline += cycle_time;

                    self.benchmark_stats.sensor_count += 1;

                    let start_gen = Instant::now();
                    let raw_data = self.generate_data();
                    self.benchmark_stats.total_gen_time += start_gen.elapsed();

                    let start_proc = Instant::now();
                    if let Some(processed_data) = self.process_data(raw_data).await {
                        self.benchmark_stats.total_proc_time += start_proc.elapsed();

                        if processed_data.anomaly {
                            let mut log = self.log.lock().await;
                                log.write(format!("[ANOMALY] {:?} ID: {}", self.sensor_type, processed_data.id));
                        }
                        // Async Send (Wait if buffer full)
                        if !self.transmit_data(&tx, processed_data).await {
                            // If transmit returns false (channel closed), stop the loop
                            break;
                        }
                    }
                }

                // EVENT 2: Received Feedback from Actuator
                Some(fb) = rx_feedback.recv() => {
                     // Check Feedback Latency
                     let latency = std::time::Instant::now().duration_since(fb.timestamp);
                     if latency > Duration::from_micros(500) {
                        self.benchmark_stats.actuator_missed_deadlines += 1;
                        let mut log = self.log.lock().await;
                         log.write(format!("[DEADLINE] Feedback for Sensor {:?} arrived late! Latency: {:?} (Limit: 500µs)",self.sensor_type, latency));
                     }

                     // Handle Recalibration
                     if fb.recalibrate_offset != 0.0 {
                         self.calibration_offset += fb.recalibrate_offset;
                         let mut log = self.log.lock().await;
                         log.write(format!("[ASYNC Feedback] Recalibrated {:?} by {:.2}", self.sensor_type, fb.recalibrate_offset));
                     }

                    // ACTION 2: Error / Alert Logging
                    // If the message is not "no", it means there is a specific warning (e.g., "Drift Detected")
                    if fb.error_msg != "no" {
                        let mut log = self.log.lock().await;
                        log.write(format!("[Feedback] Alert for {:?}: {}", self.sensor_type, fb.error_msg));
                    }
                }
            }
        }
        self.benchmark_stats
    }
}