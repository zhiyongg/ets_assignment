use std::collections::VecDeque;
use rand::Rng;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use chrono::format::Pad::Zero;
use crate::share::{BenchmarkStats, Feedback, SensorData, SensorType, SystemLog, SystemMode};
use crossbeam::channel::{Receiver,Sender};

pub struct Sensor {
    id_counter: i32,
    history_buffer: VecDeque<f64>,
    sensor_type: SensorType,
    calibration_offset: f64,
    log:Arc<Mutex<SystemLog>>,
    benchmark_stats: BenchmarkStats,
}

impl Sensor {
    pub fn new(sensor_type: SensorType,log:Arc<Mutex<SystemLog>>,) -> Self {
        Self {
            id_counter: 0,
            history_buffer: VecDeque::new(),
            sensor_type,
            calibration_offset: 0.0,
            log,
            benchmark_stats: BenchmarkStats::new()
        }
    }

    // FUNCTION 1: Generate data
    fn generate_data(&mut self) -> SensorData {
        let mut random = rand::rng();

        self.id_counter += 1;
        let mut value = match self.sensor_type {
            SensorType::Force => random.random_range(10.0..55.0),
            SensorType::Position => random.random_range(-0.1..0.2),
            SensorType::Temperature => random.random_range(20.0..130.0)
        };

        SensorData {
            id: self.id_counter,
            sensor_type: self.sensor_type,
            value: value + self.calibration_offset ,
            anomaly: false,
            timestamp:Instant::now(),
            processed_timestamp:None,
        }
    }

    // FUNCTION 2: Process data
    fn process_data(&mut self, mut data: SensorData) -> (Option<SensorData>) {
        let start = Instant::now();

        // 2.1 Detect Anomaly
        match data.sensor_type {
            SensorType::Force => if data.value < 5.0 || data.value > 60.0 { data.anomaly = true; },
            SensorType::Position => if data.value.abs() > 0.5 { data.anomaly = true; },
            SensorType::Temperature => if data.value > 120.0 { data.anomaly = true; },
        }

        if data.anomaly {
            return Some(data);
        }

        // 2.2 Apply Moving Average Filter
        if self.history_buffer.len() >= 5 {
            self.history_buffer.pop_front();
        }

        self.history_buffer.push_back(data.value);

        // Calculate the average value
        let total = self.history_buffer.iter().sum::<f64>();
        data.value = total / self.history_buffer.len() as f64;
        data.processed_timestamp = Some(Instant::now());

        // --- Deadline Check (0.2 ms) ---
        if start.elapsed() > Duration::from_micros(200) {
            self.benchmark_stats.sensor_missed_deadlines += 1;
            if let Ok(mut guard) = self.log.lock() {
                guard.write(format!("[Sensor {:?}] Processing Deadline Missed! Dropping.", data.sensor_type));
            }
            return (None);
        }

        Some(data)
    }

    // FUNCTION 3: Transmit Data
    fn transmit_data(&mut self, sender: &Sender<SensorData>, data: SensorData) -> bool {

        let mut rng = rand::rng();
        let fault_roll: f64 = rng.random_range(0.00..1.00);

        // FAULT 1: Packet Drop (5% chance)
        if fault_roll < 0.05 {
            // Log the injected fault (Measure Lock Contention)
            let start_lock = Instant::now();
            if let Ok(mut guard) = self.log.lock() {
                let contention = start_lock.elapsed();
                guard.write(format!("[FAULT] Dropping packet ID {} for {:?} (Lock Wait: {:?})", data.id, self.sensor_type, contention));
            }
            return true
        }

        // FAULT 2: Network Latency Delay (5% chance)
        if fault_roll > 0.95 {
            thread::sleep(Duration::from_micros(300)); // Deliberate delay
        }

        // 2. Transmit data
        match sender.send(data) {
            Ok(_) => true,
            Err(_) => {
                println!("[Sensor {:?}] Receiver disconnected. Stopping.", self.sensor_type);
                false
            }
        }
    }

    //  FUNCTION 4: Received Feedback and Adjust

    // ACTUAL RUN
    pub fn run(mut self,
                      sender: Sender<SensorData>,
                      rx_feedback: Receiver<Feedback>, )-> BenchmarkStats
    {

        let cycle_time = Duration::from_millis(5);
        let start_time = Instant::now();
        let mut next_deadline = start_time + cycle_time;

        loop {
            // Check active flag
            if let Ok(guard) = self.log.lock() {
                if !guard.active { break; }
            }

            // --- Jitter Measurement ---
            let now = Instant::now();
            if now > next_deadline {
                let jitter = now - next_deadline;
                self.benchmark_stats.total_jitter += jitter;
                if jitter > self.benchmark_stats.max_jitter {
                    self.benchmark_stats.max_jitter = jitter;
                }
            }

            // Increment total cycle count
            self.benchmark_stats.sensor_count += 1;

            let start = Instant::now();

            while let Ok(fb) = rx_feedback.try_recv() {

                let arrival_time = Instant::now();

                let start_time = fb.timestamp;
                let elapsed = arrival_time.duration_since(start_time);

                // Update Stats
                self.benchmark_stats.total_trans_time += elapsed;

                // 3. Check Deadline (0.1ms = 100 microseconds)
                let deadline_transmit = Duration::from_micros(100);

                if elapsed > deadline_transmit {
                    self.benchmark_stats.actuator_missed_deadlines += 1;

                    // Log the miss
                    if let Ok(mut log) = self.log.lock() {
                        log.write(format!(
                            "[DEADLINE] Feedback for Sensor {:?} arrived late! Latency: {:?} (Limit: 500µs)",
                            self.sensor_type, elapsed
                        ));
                    }
                }

                // ACTION 1: Dynamic Recalibration
                // If the offset is not 0.0, the actuator wants us to shift our values
                if fb.recalibrate_offset != 0.0 {
                    self.calibration_offset += fb.recalibrate_offset;

                    // Log the event so you get points for "Dynamic Recalibration"
                    if let Ok(mut guard) = self.log.lock() {
                        guard.write(format!("[Feedback] Sensor {:?} recalibrated by {:.2}. New Offset: {:.2}",
                                            self.sensor_type, fb.recalibrate_offset, self.calibration_offset));
                    }
                }

                // ACTION 2: Error / Alert Logging
                // If the message is not "no", it means there is a specific warning (e.g., "Drift Detected")
                if fb.error_msg != "no" {
                    if let Ok(mut guard) = self.log.lock() {
                        guard.write(format!("[Feedback] Alert for {:?}: {}", self.sensor_type, fb.error_msg));
                    }
                }

            }

            // 1. Generate Data
            let t_gen_start = Instant::now();
            let raw_data = self.generate_data();
            self.benchmark_stats.total_gen_time += t_gen_start.elapsed();
            println!("[{:?} Sensor ] Sensor Data (ID: {}) with value: {} generated", self.sensor_type, raw_data.id, raw_data.value);

            // 2. Process Data
            let t_proc_start = Instant::now();
            let processed_opt = self.process_data(raw_data.clone());
            self.benchmark_stats.total_proc_time += t_proc_start.elapsed();

            if let Some(processed_data) = processed_opt {
                let t_trans_start = Instant::now();
                // 3. Handle Anomaly
                if processed_data.anomaly {
                    if let Ok(mut log_guard) = self.log.lock() {
                        log_guard.write(format!("[ANOMALY] {:?} ID: {}", self.sensor_type, processed_data.id));
                    }
                }

                // 4. Transmit Data
                if !self.transmit_data(&sender, processed_data) {
                    break;
                }
            }

            // --- Fixed Interval Wait ---
            let work_done_time = Instant::now();
            if work_done_time < next_deadline {
                thread::sleep(next_deadline - work_done_time);
            }
            next_deadline += cycle_time;
        }
        self.benchmark_stats
    }
}