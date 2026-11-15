//! Multi-frame Differential Oracle comparing upstream reference traces
//! (`sk82_na_m.xex`, Godot C++ reference) against Foreplay simulation frames.

use serde::{Deserialize, Serialize};

/// Per-frame comparison result against an upstream binary/trace.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct FrameDifferentialVerdict {
    pub frame_index: u64,
    pub max_position_delta_m: f32,
    pub max_velocity_delta_mps: f32,
    pub within_epsilon: bool,
}

impl FrameDifferentialVerdict {
    pub fn evaluate(frame_index: u64, pos_err: f32, vel_err: f32, epsilon: f32) -> Self {
        Self {
            frame_index,
            max_position_delta_m: pos_err,
            max_velocity_delta_mps: vel_err,
            within_epsilon: pos_err <= epsilon && vel_err <= epsilon,
        }
    }
}

/// A captured snapshot of entity state at a specific simulation frame.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct OracleFrameSample {
    pub frame_index: u64,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    #[serde(default)]
    pub rotation: [f32; 4],
    #[serde(default)]
    pub state_tag: Option<String>,
}

impl OracleFrameSample {
    pub fn new(frame_index: u64, position: [f32; 3], velocity: [f32; 3]) -> Self {
        Self {
            frame_index,
            position,
            velocity,
            rotation: [0.0, 0.0, 0.0, 1.0],
            state_tag: None,
        }
    }
}

/// Aggregate report evaluating sequences of upstream debug trace frames against candidate frames.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DifferentialReport {
    pub total_frames: usize,
    pub passed_frames: usize,
    pub pass_rate_percent: f32,
    pub max_position_drift: f32,
    pub max_velocity_drift: f32,
    pub max_drift_frame: Option<u64>,
    pub position_rms_error: f32,
    pub velocity_rms_error: f32,
    pub verdicts: Vec<FrameDifferentialVerdict>,
}

impl DifferentialReport {
    /// True if 100% of compared frames satisfy the epsilon tolerance.
    pub fn is_perfect_match(&self) -> bool {
        self.total_frames > 0 && self.passed_frames == self.total_frames
    }
}

/// Evaluates a sequence of upstream reference frames against candidate simulation frames.
pub fn evaluate_trace(
    upstream: &[OracleFrameSample],
    candidate: &[OracleFrameSample],
    epsilon: f32,
) -> DifferentialReport {
    let frame_count = upstream.len().min(candidate.len());
    if frame_count == 0 {
        return DifferentialReport {
            total_frames: 0,
            passed_frames: 0,
            pass_rate_percent: 100.0,
            max_position_drift: 0.0,
            max_velocity_drift: 0.0,
            max_drift_frame: None,
            position_rms_error: 0.0,
            velocity_rms_error: 0.0,
            verdicts: Vec::new(),
        };
    }

    let mut verdicts = Vec::with_capacity(frame_count);
    let mut passed_count = 0usize;
    let mut max_pos_drift = 0.0f32;
    let mut max_vel_drift = 0.0f32;
    let mut max_drift_frame = None;
    let mut sum_sq_pos = 0.0f64;
    let mut sum_sq_vel = 0.0f64;

    for i in 0..frame_count {
        let u = &upstream[i];
        let c = &candidate[i];

        let dx = u.position[0] - c.position[0];
        let dy = u.position[1] - c.position[1];
        let dz = u.position[2] - c.position[2];
        let pos_delta = (dx * dx + dy * dy + dz * dz).sqrt();

        let dvx = u.velocity[0] - c.velocity[0];
        let dvy = u.velocity[1] - c.velocity[1];
        let dvz = u.velocity[2] - c.velocity[2];
        let vel_delta = (dvx * dvx + dvy * dvy + dvz * dvz).sqrt();

        sum_sq_pos += (pos_delta as f64) * (pos_delta as f64);
        sum_sq_vel += (vel_delta as f64) * (vel_delta as f64);

        if pos_delta > max_pos_drift {
            max_pos_drift = pos_delta;
            max_drift_frame = Some(u.frame_index);
        }
        if vel_delta > max_vel_drift {
            max_vel_drift = vel_delta;
            if max_drift_frame.is_none() {
                max_drift_frame = Some(u.frame_index);
            }
        }

        let verdict = FrameDifferentialVerdict::evaluate(u.frame_index, pos_delta, vel_delta, epsilon);
        if verdict.within_epsilon {
            passed_count += 1;
        }
        verdicts.push(verdict);
    }

    let pos_rms = (sum_sq_pos / frame_count as f64).sqrt() as f32;
    let vel_rms = (sum_sq_vel / frame_count as f64).sqrt() as f32;
    let pass_rate = (passed_count as f32 / frame_count as f32) * 100.0;

    DifferentialReport {
        total_frames: frame_count,
        passed_frames: passed_count,
        pass_rate_percent: pass_rate,
        max_position_drift: max_pos_drift,
        max_velocity_drift: max_vel_drift,
        max_drift_frame,
        position_rms_error: pos_rms,
        velocity_rms_error: vel_rms,
        verdicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_trace_perfect_match() {
        let upstream = vec![
            OracleFrameSample::new(0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            OracleFrameSample::new(1, [0.0, 1.0, 0.0], [0.0, 10.0, 0.0]),
        ];
        let candidate = upstream.clone();

        let report = evaluate_trace(&upstream, &candidate, 0.001);
        assert!(report.is_perfect_match());
        assert_eq!(report.pass_rate_percent, 100.0);
        assert_eq!(report.max_position_drift, 0.0);
        assert_eq!(report.position_rms_error, 0.0);
    }

    #[test]
    fn oracle_trace_detects_drift() {
        let upstream = vec![
            OracleFrameSample::new(0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            OracleFrameSample::new(1, [0.0, 1.0, 0.0], [0.0, 10.0, 0.0]),
        ];
        let candidate = vec![
            OracleFrameSample::new(0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            OracleFrameSample::new(1, [0.0, 1.05, 0.0], [0.0, 10.0, 0.0]),
        ];

        let report = evaluate_trace(&upstream, &candidate, 0.01);
        assert!(!report.is_perfect_match());
        assert_eq!(report.total_frames, 2);
        assert_eq!(report.passed_frames, 1);
        assert_eq!(report.pass_rate_percent, 50.0);
        assert!((report.max_position_drift - 0.05).abs() < 0.001);
        assert_eq!(report.max_drift_frame, Some(1));
    }

    #[test]
    fn parses_cleanroom_port_oracle_trace_json_and_evaluates() {
        let trace_json = include_str!("../../../templates/cleanroom-port/oracle_trace.json");
        let upstream: Vec<OracleFrameSample> = serde_json::from_str(trace_json).expect("valid oracle trace json");
        assert_eq!(upstream.len(), 60);

        // Self-evaluation must pass 100%
        let report = evaluate_trace(&upstream, &upstream, 0.0001);
        assert!(report.is_perfect_match());
        assert_eq!(report.passed_frames, 60);
        assert_eq!(report.position_rms_error, 0.0);
    }
}
