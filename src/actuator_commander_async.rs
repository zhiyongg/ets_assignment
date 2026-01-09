use std::collections::HashMap;
use tokio::sync::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::time::{Instant, Duration};
use crate::share::{BenchmarkStats, PidController, SensorData, SensorType, SystemLog, SystemMode};

pub struct ActuatorCommanderAsync {
    pids: HashMap<SensorType, PidController>,
    sender_actuators: HashMap<SensorType, Sender<SensorData>>,
    log: Arc<Mutex<SystemLog>>,
    system_mode: SystemMode,
    consecutive_anomalies: u32,
    benchmark_stats: BenchmarkStats,
}

impl ActuatorCommanderAsync {
    pub fn new(
        sender_actuators: HashMap<SensorType, Sender<SensorData>>,
        log: Arc<Mutex<SystemLog>>,
    ) -> Self {
        let mut pids = HashMap::new();
        pids.insert(SensorType::Force, PidController::new(1.5, 0.1, 0.05));
        pids.insert(SensorType::Position, PidController::new(0.8, 0.2, 0.1));
        pids.insert(SensorType::Temperature, PidController::new(0.5, 0.05, 0.01));

        Self {
            pids,
            sender_actuators,
            log,
            system_mode: SystemMode::Normal,
            consecutive_anomalies: 0,
            benchmark_stats: BenchmarkStats::new(),
        }
    }

    async fn handle_sensor_data(&mut self, mut data: SensorData) {
        let arrival_time = std::time::Instant::now(); // Use Std Instant for duration math with data.timestamp

        // 1. Stats & Deadline
        if let Some(start_time) = data.processed_timestamp {
            let elapsed = arrival_time.duration_since(start_time);
            self.benchmark_stats.total_trans_time += elapsed;
            if elapsed > Duration::from_micros(100) {
                self.benchmark_stats.sensor_missed_deadlines += 1;
            }
        }

        // 2. PID Control Logic
        let setpoint = match data.sensor_type {
            SensorType::Force => 30.0,
            SensorType::Position => 0.0,
            SensorType::Temperature => 240.0,
        };

        if let Some(pid) = self.pids.get_mut(&data.sensor_type) {
            let scale = if self.system_mode == SystemMode::Degraded { 0.5 } else { 1.0 };
            let effort = pid.compute(setpoint, data.value, 0.005, scale);
            data.value = effort; // Update data with control effort

            // 3. Forward to Actuator (Async Send)
            if let Some(tx) = self.sender_actuators.get(&data.sensor_type) {
                let _ = tx.send(data).await;
            }
        }

        self.benchmark_stats.total_actuator_time += arrival_time.elapsed();
    }

    pub async fn run(
        mut self,
        mut rx_force: Receiver<SensorData>,
        mut rx_pos: Receiver<SensorData>,
        mut rx_temp: Receiver<SensorData>,
    ) -> BenchmarkStats {

        // Loop continuously waiting for ANY of the 3 sensors
        loop {
            tokio::select! {
                Some(data) = rx_force.recv() => self.handle_sensor_data(data).await,
                Some(data) = rx_pos.recv() => self.handle_sensor_data(data).await,
                Some(data) = rx_temp.recv() => self.handle_sensor_data(data).await,
                else => break, // If all channels close, exit
            }
        }
        self.benchmark_stats
    }
}