use std::collections::VecDeque;
use tokio::time::{self, Duration, timeout};
use tokio::sync::{Mutex, mpsc, broadcast};
use rand::Rng;
use std::sync::Arc;
use std::time::Instant;
use crate::share::{Feedback, SensorData, SensorType, SystemLog};

struct Sensor {
    id_counter:i32,
    history_buffer:VecDeque<f64>,
    sensor_type: SensorType,
}

impl Sensor {
    fn new(sensor_type: SensorType) -> Self {
        Self {
            id_counter: 0,
            history_buffer: VecDeque::new(),
            sensor_type,
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
            value,
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
    async fn transmit_data(&self, tx: mpsc::Sender<SensorData>, data: SensorData) {

        // Deadline for transmit data
        let deadline_transmit = Duration::from_micros(100);

        let result = timeout(deadline_transmit, tx.send(data)).await.unwrap();

        match result {
            Ok(_) => println!("transmit data within the time!"),
            Err(_) => println!("OUT OF TIME: TASK ABANDONED")
        }
    }
    
}

pub async fn run_sensor(
    sensor_type: SensorType,
    tx: mpsc::Sender<SensorData>,
    mut rx_feedback: broadcast::Receiver<Feedback>,
    log: Arc<Mutex<SystemLog>>)
    {
        let mut sensor = Sensor::new(sensor_type);
        let sampling_rate = Duration::from_millis(5);
        let mut interval = time::interval(sampling_rate);

        loop {
            tokio::select! {
                    _ = interval.tick()=>{
                        // 1. Generate data
                        let raw_data = sensor.generate_data();

                        // 2. Process data
                        if let Some(processed_data) = sensor.process_data(raw_data) {

                            // 3. Handle Anomaly Logging
                            if processed_data.anomaly {
                                let mut log_guard = log.lock().await;
                                log_guard.write(format!("[ANOMALY] {:?} Sensor Value: {:.2}", sensor.sensor_type, processed_data.value));
                                // We still send anomalies so actuator can E-STOP
                            }

                            // 4. Transmit data
                            sensor.transmit_data(tx.clone(), processed_data).await;
                        }
                    }
                }
        }

        // 5. Handle feedback

    }