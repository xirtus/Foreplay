//! `PortHost` — Clean-room game port hosting (`skate2-rust-port` BYO-ROM + `rustports.org` differential oracle).

/// Per-frame comparison result against an upstream binary/trace (`sk82_na_m.xex`, Godot C++ reference, etc.).
#[derive(Clone, PartialEq, Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differential_oracle_flags_drift_above_epsilon() {
        let pass = FrameDifferentialVerdict::evaluate(42, 0.0001, 0.0002, 0.001);
        assert!(pass.within_epsilon);

        let drift = FrameDifferentialVerdict::evaluate(43, 0.05, 0.0, 0.001);
        assert!(!drift.within_epsilon);
    }
}
