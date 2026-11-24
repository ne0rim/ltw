//! Health and life state.

use bevy_ecs::component::Component;

#[derive(Component, Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Default for Health {
    fn default() -> Self {
        Self {
            current: 100.0,
            max: 100.0,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LifeState {
    #[default]
    Healthy,
    Wounded,
    Incapacitated,
    Dead,
}

impl LifeState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Wounded => "wounded",
            Self::Incapacitated => "incapacitated",
            Self::Dead => "dead",
        }
    }
    /// Derive the state from health (thresholds will become data later).
    pub fn from_health(h: &Health) -> Self {
        let f = h.current / h.max.max(1.0);
        if f <= 0.0 {
            Self::Dead
        } else if f < 0.25 {
            Self::Incapacitated
        } else if f < 1.0 {
            Self::Wounded
        } else {
            Self::Healthy
        }
    }
}
