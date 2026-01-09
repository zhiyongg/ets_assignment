use std::time::{Duration, Instant};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

// --------------- SENSOR MODULE -------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SensorType { Force, Position, Temperature }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemMode {
    Normal,
    Degraded,      // Slow down operation
    EmergencyStop, // Halt and hold safe position
}

#[derive(Debug, Clone)]
pub struct SensorData {
    pub id: i32,
    pub sensor_type: SensorType,
    pub value: f64,
    pub anomaly: bool,
    pub timestamp: Instant,
    pub processed_timestamp: Option<Instant>,
}

#[derive(Debug, Clone)]
pub enum SensorFeedback {
    Recalibrate { offset: f64 }, // Instruct sensor to shift values
    Maintain,
}

// Inefficient Struct Approach
pub struct Feedback {
    pub is_ack: bool,
    pub error_msg: String,
    pub recalibrate_offset: f64,
    pub timestamp: Instant,
}

// --------------- ACTUATOR COMMANDER MODULE -------------------

#[derive(Debug, Clone)]
pub enum ActuatorStatus {
    ActionComplete { sensor_type: SensorType, effort: f64 },
    HardwareFailure(String),
}


// --------------- PID CONTROLLER -------------------
#[derive(Clone)]
pub struct PidController {
    pub kp: f64, pub ki: f64, pub kd: f64,
    pub integral: f64, pub prev_error: f64,
}

impl PidController {
    pub fn new(kp: f64, ki: f64, kd: f64) -> Self {
        Self { kp, ki, kd, integral: 0.0, prev_error: 0.0 }
    }
    pub fn compute(&mut self, target: f64, current: f64, dt: f64, scale: f64) -> f64 {
        let error = target - current;
        self.integral += error * dt;
        let derivative = (error - self.prev_error) / dt;
        self.prev_error = error;
        ((self.kp * error) + (self.ki * self.integral) + (self.kd * derivative)) * scale
    }
}

// --------------- LOG FILE -------------------
pub struct SystemLog {
    file: Option<File>,
    pub active: bool,
}

impl SystemLog {
    pub fn new() -> Self {
        // Create/Truncate simulation.txt
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("system.log")
            .expect("Unable to open log file");

        Self {
            file: Some(file),
            active: true,
        }
    }

    pub fn write(&mut self, msg: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S%.3f"); // Requires 'chrono' crate, or use debug formatting
        let log_line = format!("[{}] {}\n", timestamp, msg);

        // Write to file instead of println
        if let Some(ref mut file) = self.file {
            let _ = file.write_all(log_line.as_bytes());
        }
    }
    pub fn alert(&mut self, msg: String) {
        let banner = format!("\n**************************************************\n!!! {} !!!\n**************************************************\n", msg);
        println!("{}", banner); // Force print to console
        self.write(msg); // Log to file
    }
}

// --------------- BENCHMARK -------------------
#[derive(Debug, Default, Clone, Copy)]
pub struct BenchmarkStats {
    pub sensor_count: u32,
    pub actuator_count: u32,
    pub total_actuator_time:Duration,
    pub total_gen_time: Duration,
    pub total_proc_time: Duration,
    pub total_trans_time: Duration,
    pub total_jitter: Duration,
    pub max_jitter: Duration,
    pub total_at_jitter: Duration,
    pub max_at_jitter: Duration,
    pub total_latency: Duration,
    pub sensor_missed_deadlines: u32,
    pub actuator_missed_deadlines: u32,
}

impl BenchmarkStats {
    pub fn new() -> Self { Self::default() }
    pub fn avg_gen(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_gen_time / self.sensor_count } }
    pub fn avg_proc(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_proc_time / self.sensor_count } }
    pub fn avg_trans(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_trans_time / self.sensor_count } }
    pub fn avg_jitter(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_jitter / self.sensor_count } }
    pub fn avg_at_jitter(&self) -> Duration { if self.actuator_count == 0 { Duration::ZERO } else { self.total_at_jitter / self.actuator_count } }

    pub fn avg_actuator(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_actuator_time / self.sensor_count } }
    pub fn avg_latency(&self) -> Duration { if self.sensor_count == 0 { Duration::ZERO } else { self.total_latency / self.sensor_count } }
    pub fn throughput(&self, total_run_time: Duration) -> f64 {
        if total_run_time.as_secs_f64() == 0.0 { 0.0 } else { self.sensor_count as f64 / total_run_time.as_secs_f64() }
    }

    pub fn sensor_deadline_rate (&self) -> f64 {
        (self.sensor_missed_deadlines as f64 / self.sensor_count as f64) * 100.0
    }

    pub fn actuator_deadline_rate (&self) -> f64 {
        (self.actuator_missed_deadlines as f64 / self.actuator_count as f64) * 100.0
    }

    pub fn merge(&mut self, other: &BenchmarkStats) {
        self.sensor_count += other.sensor_count;
        self.actuator_count += other.actuator_count;
        self.sensor_missed_deadlines += other.sensor_missed_deadlines;
        self.actuator_missed_deadlines += other.actuator_missed_deadlines;
        self.total_gen_time += other.total_gen_time;
        self.total_proc_time += other.total_proc_time;
        self.total_trans_time += other.total_trans_time;
        self.total_jitter += other.total_jitter;
        self.total_at_jitter += other.total_at_jitter;
        self.max_jitter = self.max_jitter.max(other.max_jitter);
        self.max_at_jitter = self.max_at_jitter.max(other.max_at_jitter);
        self.total_actuator_time += other.total_actuator_time;
        self.total_latency += other.total_latency;
    }
}