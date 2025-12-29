use std::collections::HashMap;
use std::process::Command;
use tokio::sync::{mpsc, broadcast, Mutex};
use tokio::time::{self, Duration, Instant, timeout};
use std::sync::Arc;
use crate::share::{Feedback, PidController, SensorData, SensorType, SystemLog};

#[derive(Debug, Clone)]
pub struct ActuatorCommand {
    pub actuator: SensorType,
    pub target_value: f64,
    pub timestamp: Instant,
}

// 3. PERFORMANCE MONITORING: For Benchmarking & Analysis
struct ActuatorStats {
    total_tasks: u64,
    missed_deadlines: u64,
    total_latency_us: u128,
}

impl ActuatorStats {
    fn new() -> Self {
        Self { total_tasks: 0, missed_deadlines: 0, total_latency_us: 0 }
    }

    fn update(&mut self, latency: Duration, missed: bool) {
        self.total_tasks += 1;
        self.total_latency_us += latency.as_micros();
        if missed { self.missed_deadlines += 1; }
    }

    fn print_report(&self) {
        if self.total_tasks == 0 { return; }
        println!("\n=== ACTUATOR PERFORMANCE ANALYSIS ===");
        println!("Throughput:       {} tasks in simulation period", self.total_tasks);
        println!("Missed Deadlines: {}", self.missed_deadlines);
        println!("Avg Latency:      {:.2} µs", (self.total_latency_us as f64 / self.total_tasks as f64));
    }
}


// ACTUATOR COMMANDER
struct ActuatorCommander {
    pids: HashMap<SensorType, PidController>,
    stats: ActuatorStats,
    tx_feedback: broadcast::Sender<Feedback>,
    log: Arc<Mutex<SystemLog>>,
}

impl ActuatorCommander {
    fn new(tx_feedback: broadcast::Sender<Feedback>, log: Arc<Mutex<SystemLog>>) -> Self {
        let mut pids = HashMap::new();
        // Priority-based scheduling gains
        pids.insert(SensorType::Force, PidController::new(1.5, 0.1, 0.05));
        pids.insert(SensorType::Position, PidController::new(0.8, 0.2, 0.1));
        pids.insert(SensorType::Temperature, PidController::new(0.5, 0.05, 0.01));

        Self { pids, stats: ActuatorStats::new(), tx_feedback, log }
    }

    async fn process_sensor_data(&mut self, data: SensorData) {
        let start_time = Instant::now();

        // 1. FAULT TOLERANCE (CRITICAL PRIORITY)
        if data.anomaly {
            let _ = self.tx_feedback.send(Feedback::EmergencyStop);
            let mut l = self.log.lock().await;
            l.write(format!("[E-STOP] Anomaly detected on {:?}. System halted.", data.sensor_type));
            return;
        }

        // 2. PID3
        let setpoint = match data.sensor_type {
            SensorType::Force => 30.0,
            SensorType::Position => 0.0,
            SensorType::Temperature => 240.0,
        };

        if let Some(pid) = self.pids.get_mut(&data.sensor_type) {
            let _effort = pid.compute(setpoint, data.value, 0.005);
        }

        // 3. EXECUTE VIRTUAL ACTUATION
        // Spawning tasks ensures multiple actuators run concurrently without blocking.
        let sensor_type = data.sensor_type;
        let log_clone = self.log.clone();

        tokio::spawn(async move {
            let (exec_time, deadline) = match sensor_type {
                SensorType::Force       => (Duration::from_micros(500), Duration::from_millis(2)),
                SensorType::Position    => (Duration::from_micros(500), Duration::from_millis(2)),
                SensorType::Temperature => (Duration::from_micros(500), Duration::from_millis(2)),
            };

            time::sleep(exec_time).await; // Simulated Work

            if start_time.elapsed() > deadline {
                let mut l = log_clone.lock().await;
                l.write(format!("[DEADLINE MISS] {:?} Delay: {:?}", sensor_type, start_time.elapsed()));
            }
        });

        // 4. FEEDBACK
        // Must send feedback within 0.5ms of actuation logic completion.
        let feedback_task = async {
            if (setpoint - data.value).abs() > 10.0 {
                let _ = self.tx_feedback.send(Feedback::Recalibrate(data.sensor_type));
            } else {
                let _ = self.tx_feedback.send(Feedback::Ack(data.id));
            }
        };

        if timeout(Duration::from_micros(500), feedback_task).await.is_err() {
            let mut l = self.log.lock().await;
            l.write("[TIMEOUT] Feedback deadline missed!".to_string());
        }

        self.stats.update(start_time.elapsed(), start_time.elapsed() > Duration::from_millis(2));
    }

    pub async fn run(
        mut self,
        mut rx1: mpsc::Receiver<SensorData>,
        mut rx2: mpsc::Receiver<SensorData>,
        mut rx3: mpsc::Receiver<SensorData>,
    ) {
        loop {
            tokio::select! {
                Some(data) = rx1.recv() => self.process_sensor_data(data).await,
                Some(data) = rx2.recv() => self.process_sensor_data(data).await,
                Some(data) = rx3.recv() => self.process_sensor_data(data).await,
                else => break,
            }
        }
        self.stats.print_report();
    }
}

pub async fn run_actuator(
    rx1: mpsc::Receiver<SensorData>,
    rx2: mpsc::Receiver<SensorData>,
    rx3: mpsc::Receiver<SensorData>,
    tx_feedback: broadcast::Sender<Feedback>,
    log: Arc<Mutex<SystemLog>>
) {
    let commander = ActuatorCommander::new(tx_feedback, log);
    commander.run(rx1, rx2, rx3).await;
}