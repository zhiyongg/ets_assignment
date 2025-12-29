use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensorType { Force, Position, Temperature }

#[derive(Debug, Clone)]
pub struct SensorData {
    pub id: i32,
    pub sensor_type: SensorType,
    pub value: f64,
    pub anomaly: bool,
    pub timestamp: Instant,
}

#[derive(Clone)]
pub struct PidController {
    pub kp: f64, pub ki: f64, pub kd: f64,
    pub integral: f64, pub prev_error: f64,
}

impl PidController {
    pub fn new(kp: f64, ki: f64, kd: f64) -> Self {
        Self { kp, ki, kd, integral: 0.0, prev_error: 0.0 }
    }
    pub fn compute(&mut self, target: f64, current: f64, dt: f64) -> f64 {
        let error = target - current;
        self.integral += error * dt;
        let derivative = (error - self.prev_error) / dt;
        self.prev_error = error;
        (self.kp * error) + (self.ki * self.integral) + (self.kd * derivative)
    }
}

#[derive(Debug, Clone)]
pub enum Feedback {
    Ack(i32),
    Recalibrate(SensorType),
    EmergencyStop,
}

#[derive(Debug, Clone)]
pub struct FeedbackData {
    pub feedback_id: u8,              // Target sensor
    pub timestamp_us: u64,          // Feedback generation time
    pub control_error: f32,         // PID error
    pub instruction: f32, // What sensor should do
}

pub struct SystemLog {
    pub entries: Vec<String>,
    pub active: bool // CHANGED: () -> bool
}

impl SystemLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            active: true // CHANGED: Initialize as true (system is active)
        }
    }

    pub fn write(&mut self, msg: String) {
        self.entries.push(msg);
    }
}

#[derive(Debug, Clone)]
pub enum ActuatorStatus {
    ActionComplete { sensor_type: SensorType, effort: f64 },
    HardwareFailure(String),
}

#[derive(Debug, Clone)]
pub enum SensorFeedback {
    Recalibrate { offset: f64 }, // Instruct sensor to shift values
    Maintain,
}