use std::sync::{mpsc, Arc, Mutex};
use std::collections::HashMap;
use crossbeam::channel::{self, Receiver, Sender, select};
use std::thread;
use std::thread::Thread;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use crate::share::{ActuatorStatus, Feedback, PidController, SensorData, SensorFeedback, SensorType, SystemLog};

pub struct ActuatorCommander {
    pids: HashMap<SensorType, PidController>,
    log: Arc<Mutex<SystemLog>>,
    actuator_channels: HashMap<SensorType, Sender<f64>>,
    sensor_feedback_tx: HashMap<SensorType,Sender<SensorFeedback>>,
}

impl ActuatorCommander {
    pub fn new(sensor_feedback_tx: HashMap<SensorType,Sender<SensorFeedback>>, log: Arc<Mutex<SystemLog>>) -> Self {
        let mut pids = HashMap::new();

        pids.insert(SensorType::Force, PidController::new(1.5, 0.1, 0.05));
        pids.insert(SensorType::Position, PidController::new(0.8, 0.2, 0.1));
        pids.insert(SensorType::Temperature, PidController::new(0.5, 0.05, 0.01));
        Self { pids, sensor_feedback_tx, log, actuator_channels: HashMap::new() }
    }


    fn process_data(&mut self, data:SensorData){
        let start = Instant::now();

        // 1. Fault Tolerance
        if data.anomaly{
            // let _ = self.tx_feedback.send(Feedback::EmergencyStop);
            let mut log = self.log.lock().unwrap();
            log.write(format!("[E-STOP] Anomaly detected on {:?}. System halted.", data.sensor_type));
            return;
        }

        // 2. PID
        let setpoint = match data.sensor_type {
            SensorType::Force => 30.0,
            SensorType::Position => 0.0,
            SensorType::Temperature => 240.0,
        };

        if let Some(pid) = self.pids.get_mut(&data.sensor_type){
            let effort = pid.compute(setpoint, data.value, 0.005);

            // --- Route to Specific Actuator ---
            if let Some(actuator_tx) = self.actuator_channels.get(&data.sensor_type) {
                // Send the command to the specific thread
                actuator_tx.send(effort).unwrap();
            }
        }

    }

    fn handle_message(&mut self, msg_result: Result<SensorData, channel::RecvError>)->bool {
        match msg_result {
            Ok(data) => {
                self.process_data(data);
                true // Continue running
            }
            Err(_) => {
                // channel is closed, we should stop to avoid infinite loop
                false // Stop running
            }
        }
    }

    fn spawn_actuators(&mut self) -> Receiver<ActuatorStatus> {
        let (status_tx, status_rx) = channel::unbounded();

        let actuators = vec![
            ("Motor", SensorType::Force),
            ("Gripper", SensorType::Position),
            ("Stabiliser", SensorType::Temperature),
        ];

        for (name, sensor_type) in actuators {
            let (cmd_tx, cmd_rx) = channel::unbounded();
            self.actuator_channels.insert(sensor_type, cmd_tx);

            // Clone the status transmitter for this actuator
            let my_status_tx = status_tx.clone();

            let mut actuator = Actuator::new(name.to_string(), sensor_type);

            thread::spawn(move || {
                actuator.run(cmd_rx, my_status_tx);
            });
        }
        status_rx // Return the receiver so Commander can listen
    }

    fn process_actuator_status(&mut self, status: ActuatorStatus) {
        match status {
            ActuatorStatus::ActionComplete { sensor_type, effort } => {
                // Logic: If effort was huge, tell sensor to recalibrate (simulated)
                if effort.abs() > 10.0 {
                    if let Some(fb_tx) = self.sensor_feedback_tx.get(&sensor_type) {
                        // Send feedback to SENSOR
                        let _ = fb_tx.send(SensorFeedback::Recalibrate { offset: -1.0 });
                        // println!("Feedback sent to {:?}", sensor_type);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn run(
        mut self,
        rx_force: Receiver<SensorData>,
        rx_pos: Receiver<SensorData>,
        rx_temp: Receiver<SensorData>, )
    {

        let rx_status = self.spawn_actuators();

        println!("Commander: System Active. Listening for sensor data...");

        let mut active = true;

        while active {
            select! {
                recv(rx_force) -> msg => { active = self.handle_message(msg); },
                recv(rx_pos) -> msg => { active = self.handle_message(msg); },
                recv(rx_temp) -> msg => { active = self.handle_message(msg); },
            }

            // Optional: Also check the SystemLog active flag to be safe
            if let Ok(log) = self.log.lock() {
                if !log.active { break; }
            }
        }

        // loop {
        //     // CROSSBEAM SELECT!
        //     // This blocks until ANY one of the channels receives a message.
        //     select! {
        //         recv(rx_force) -> msg => self.handle_message(msg),
        //         recv(rx_pos) -> msg => self.handle_message(msg),
        //         recv(rx_temp) -> msg => self.handle_message(msg),
        //
        //         // B. Handle Actuator Status -> Send Feedback to Sensor
        //         recv(rx_status) -> msg => {
        //              if let Ok(status) = msg {
        //                  self.process_actuator_status(status);
        //              }
        //         }
        //     }
        // }
    }

}

struct Actuator{
    name: String,
    sensor_type: SensorType
}

impl Actuator{

    fn new(name: String, sensor_type: SensorType) -> Self {
        Self { name, sensor_type }
    }

    fn run(&mut self, rx:Receiver<f64>,tx_status: Sender<ActuatorStatus>){

        while let Ok(effort) = rx.recv() {
            let start = Instant::now();

            // Simulate Actuation
            println!("Actuator [{}] adjusting to effort {:.2}", self.name, effort);

            // Simulate hardware delay (1ms)
            thread::sleep(Duration::from_micros(50));

            // Optional: Check if we met the deadline (Source: 58)
            if start.elapsed() > Duration::from_millis(2) {
                println!("Warning: Actuator [{}] deadline missed!", self.name);
            }

            let _ = tx_status.send(ActuatorStatus::ActionComplete {
                sensor_type: self.sensor_type,
                effort,
            });
        }

    }


}