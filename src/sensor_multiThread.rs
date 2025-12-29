use std::collections::VecDeque;
use rand::Rng;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use crate::share::{Feedback, SensorData, SensorFeedback, SensorType, SystemLog};
use crossbeam::channel::{Receiver,Sender};

// --- Benchmark Metrics Structure ---
// Requirement 5: Record execution times for each stage
#[derive(Debug, Default, Clone, Copy)]
struct SensorMetrics {
    generation_time: Duration,
    processing_time: Duration,
    lock_wait_time: Duration,
    transmission_time: Duration,
    total_latency: Duration,
}

pub struct Sensor {
    id_counter: i32,
    history_buffer: VecDeque<f64>,
    sensor_type: SensorType,
    calibration_offset: f64,
}


impl Sensor {
    pub fn new(sensor_type: SensorType) -> Self {
        Self {
            id_counter: 0,
            history_buffer: VecDeque::new(),
            sensor_type,
            calibration_offset: 0.0,
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

        }
    }

    // FUNCTION 2: Process data
    fn process_data(&mut self, mut data: SensorData) -> Option<SensorData> {
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

        // --- Deadline Check (0.2 ms) ---
        if start.elapsed() > Duration::from_micros(200) {
            println!("[Sensor {:?}] Processing Deadline Missed! Dropping.", data.sensor_type);
            return None;
        }

        Some(data)
    }

    // FUNCTION 3: Transmit Data
    // fn transmit_data(&self, sender: Sender<SensorData>, data: SensorData) {
    //
    //     // Deadline for transmit data
    //     let deadline_transmit = Duration::from_micros(100);
    //
    //     let start = Instant::now();
    //
    //     sender.send(data.clone());
    //     let elapsed = start.elapsed();
    //     if elapsed > deadline_transmit {
    //         println!("Data(ID: {}) missed deadline {:?}", data.id, elapsed - deadline_transmit);
    //     } else {
    //         println!("Data(ID: {})  within deadline. Total Time: {:?}", data.id, start.elapsed());
    //     }
    // }
    fn transmit_data(&self, sender: &Sender<SensorData>, data: SensorData) -> Result<(), ()> {
        let deadline_transmit = Duration::from_micros(100);
        let start = Instant::now();

        // CHANGED: Removed .unwrap(), implemented error handling
        match sender.send(data.clone()) {
            Ok(_) => {
                let elapsed = start.elapsed();
                if elapsed > deadline_transmit {
                    println!("Data(ID: {}) missed deadline {:?}", data.id, elapsed - deadline_transmit);
                } else {
                    // Optional: reduce verbosity by commenting this out in production
                    // println!("Data(ID: {}) within deadline.", data.id);
                }
                Ok(())
            }
            Err(_) => {
                // The receiver was dropped (simulation ended)
                println!("[Sensor {:?}] Receiver disconnected. Stopping transmission.", self.sensor_type);
                Err(())
            }
        }
    }

    //  FUNCTION 4: Received Feedback and Adjust

    // ACTUAL RUN
    pub fn run(mut self,
                      sender: Sender<SensorData>,
                      rx_feedback: Receiver<SensorFeedback>,
                      log: Arc<Mutex<SystemLog>>)
    {

        let cycle_time = Duration::from_micros(500);

        loop{
            let start = Instant::now();

            // Check Feedback
            match rx_feedback.try_recv() {
                Ok(SensorFeedback::Recalibrate { offset }) => {
                    println!("[Sensor {:?}] Feedback received: Recalibrating...", self.sensor_type);
                    self.calibration_offset += offset;
                },
                Ok(_) => {},
                Err(_) => {} // No message, continue
            }

            // 1. Generate Data
            let raw_data = self.generate_data();

            // 2. Process Data
            if let Some(processed_data) = self.process_data(raw_data) {

                // 3. Handle Anomaly
                if processed_data.anomaly{
                    if let Ok(mut log_guard) = log.lock() {
                        println!("[ANOMALY] {:?} ID: {}", self.sensor_type, processed_data.id);
                    }
                }

                // 4. Transmit Data
                if self.transmit_data(&sender, processed_data).is_err() {
                    println!("[Sensor {:?}] Shutting down thread.", self.sensor_type);
                    break; // Exit the loop cleanly
                }
            }
            let elapsed = start.elapsed();
            if elapsed < cycle_time {
                std::thread::sleep(cycle_time - elapsed);
            }
        }
    }
}