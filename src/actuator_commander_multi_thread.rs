use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use crossbeam::{channel, select};
use crossbeam::channel::{Receiver, Sender};
use crate::share::{BenchmarkStats, Feedback, PidController, SensorData, SensorType, SystemLog, SystemMode};

pub struct ActuatorCommander {
    pids: HashMap<SensorType, PidController>,
    sender_actuators: HashMap<SensorType, Sender<SensorData>>,
    // receiver_feedbacks: HashMap<SensorType, Receiver<Feedback>>,
    // sender_feedback: HashMap<SensorType,Sender<Feedback>>,
    log:Arc<Mutex<SystemLog>>,
    system_mode: SystemMode,
    consecutive_anomalies: u32,
    benchmark_stats: BenchmarkStats,
}

impl ActuatorCommander {
    pub fn new(
        sender_actuators: HashMap<SensorType, Sender<SensorData>>,
        // receiver_feedbacks: HashMap<SensorType, Receiver<Feedback>>,
        // sender_feedback: HashMap<SensorType, Sender<Feedback>>,
        log: Arc<Mutex<SystemLog>>,
    ) -> Self {

        let mut pids = HashMap::new();
        pids.insert(SensorType::Force, PidController::new(1.5, 0.1, 0.05));
        pids.insert(SensorType::Position, PidController::new(0.8, 0.2, 0.1));
        pids.insert(SensorType::Temperature, PidController::new(0.5, 0.05, 0.01));

        Self {
            pids,
            sender_actuators,
            // receiver_feedbacks,
            // sender_feedback,
            log,
            system_mode: SystemMode::Normal,
            consecutive_anomalies: 0,
            benchmark_stats: BenchmarkStats::new(),
        }
    }

    // FUNCTION 1: Record Jitter


    // FUNCTION 2: Handle received data
    fn handle_sensor_data(&mut self, mut data:SensorData) {
        // 1. Capture Reception Time immediately
        let arrival_time = Instant::now();

        if let Some(start_time) = data.processed_timestamp {
            let elapsed = arrival_time.duration_since(start_time);

            // Update Stats
            self.benchmark_stats.total_trans_time += elapsed;

            // 3. Check Deadline (0.1ms = 100 microseconds)
            let deadline_transmit = Duration::from_micros(100);

            if elapsed > deadline_transmit {
                self.benchmark_stats.sensor_missed_deadlines += 1;

                // Log the miss
                if let Ok(mut log) = self.log.lock() {
                    log.write(format!(
                        "[DEADLINE] Sensor {:?} (ID: {}) took {:?} (limit: 100µs)",
                        data.sensor_type, data.id, elapsed
                    ));
                }
            }
        }

        // 2.1 Perform PID
        let setpoint = match data.sensor_type {
            SensorType::Force => 30.0,
            SensorType::Position => 0.0,
            SensorType::Temperature => 240.0,
        };

        if let Some(pid) = self.pids.get_mut(&data.sensor_type) {
            let scale = if self.system_mode == SystemMode::Degraded { 0.5 } else { 1.0 };
            let effort = pid.compute(setpoint, data.value, 0.005,scale);
            data.value = effort;

            // 2.2 Send data to specific actuator
            self.send_command(data.sensor_type, data);
        }

        let duration = arrival_time.elapsed();
        self.benchmark_stats.total_actuator_time += duration;

    }

    // FUNCTION 3: Send command to actuator
    fn send_command(&self, s_type: SensorType, data: SensorData) {
        if let Some(tx) = self.sender_actuators.get(&s_type) {
            let _ = tx.send(data);
        }
    }

    // FUNCTION 4: Write System Log
    fn log_status(&self, msg: String) {
        if let Ok(mut log) = self.log.lock() {
            log.write(msg);
        }
    }

    // FUNCTION 5: Send feedback to sensor
    // fn handle_feedback(&self, s_type: SensorType, feedback: Feedback) {
    //     if let Some(tx) = self.sender_feedback.get(&s_type) {
    //         let _ = tx.send(feedback);
    //     }
    // }

    // FUNCTION 6: Fail-Safe Mode
    pub fn fail_safe(&mut self, data:SensorData) {
        // 1. Fault Tolerance
        if data.anomaly {
            self.consecutive_anomalies += 1;

            // Case 1: Switch to Degraded
            if self.consecutive_anomalies >= 3 && self.system_mode != SystemMode::EmergencyStop {
                self.system_mode = SystemMode::Degraded;
                if let Ok(mut log) = self.log.lock() {
                    log.alert("High Anomaly Rate! Switching to DEGRADED MODE.".to_string());
                }
            }
            // Case 2: Switch to E-STOP
            if self.consecutive_anomalies >= 10 {
                self.system_mode = SystemMode::EmergencyStop;
                if let Ok(mut log) = self.log.lock() {
                    log.alert("CRITICAL FAILURE! Switching to E-STOP.".to_string());
                }
            } else {
                // Recovery logic
                if self.consecutive_anomalies > 0 { self.consecutive_anomalies -= 1; }
                if self.consecutive_anomalies == 0 && self.system_mode == SystemMode::Degraded {
                    self.system_mode = SystemMode::Normal;
                    if let Ok(mut log) = self.log.lock() {
                        log.alert("System Stabilized. Returning to NORMAL MODE.".to_string());
                    }
                }
            }

            // 3. // --- Control Logic ---
            if self.system_mode == SystemMode::EmergencyStop {
                self.send_command(data.sensor_type, data);
                return;
            }

        }
    }

    pub fn run(
        mut self,
        rx_force: Receiver<SensorData>,
        rx_pos: Receiver<SensorData>,
        rx_temp: Receiver<SensorData>, ) -> BenchmarkStats
    {

        // 1. Set up for the feedback receiver
        // let rx_fb_force = self.receiver_feedbacks.get(&SensorType::Force).expect("Force FB missing").clone();
        // let rx_fb_pos = self.receiver_feedbacks.get(&SensorType::Position).expect("Pos FB missing").clone();
        // let rx_fb_temp = self.receiver_feedbacks.get(&SensorType::Temperature).expect("Temp FB missing").clone();

        let mut active = true;

        let start_run = Instant::now();

        while active {
            select! {
                // --- SENSOR INPUTS ---
                recv(rx_force) -> msg => {
                    match msg {
                        Ok(data) => self.handle_sensor_data(data),
                        Err(_) => active = false, // Stop if channel disconnects
                    }
                },
                recv(rx_pos) -> msg => {
                    match msg {
                        Ok(data) => self.handle_sensor_data(data),
                        Err(_) => active = false,
                    }
                },
                recv(rx_temp) -> msg => {
                    match msg {
                        Ok(data) => self.handle_sensor_data(data),
                        Err(_) => active = false,
                    }
                },
                //default(Duration::from_millis(100)) => {}


                // --- ACTUATOR FEEDBACK---
                // recv(rx_fb_force) -> msg => {
                //     match msg {
                //         Ok(fb) => self.handle_feedback(SensorType::Force, fb),
                //         Err(_) => active = false,
                //     }
                // },
                // recv(rx_fb_pos) -> msg => {
                //     match msg {
                //         Ok(fb) => self.handle_feedback(SensorType::Position, fb),
                //         Err(_) => active = false,
                //     }
                // },
                // recv(rx_fb_temp) -> msg => {
                //     match msg {
                //         Ok(fb) => self.handle_feedback(SensorType::Temperature, fb),
                //         Err(_) => active = false,
                //     }
                // },

            }

            if let Ok(log) = self.log.lock() {
                if !log.active { break; }
            }
        }

        self.benchmark_stats
    }
}