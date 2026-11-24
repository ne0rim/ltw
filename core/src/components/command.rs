//! Who fights for whom, who commands a unit, and what a unit has been tasked
//! to do.
//!
//! Every unit, at every level, has a `Controller`: the script, the player or
//! the AI. All three give the same kinds of instruction:
//!
//! - a `Task` says *what* to achieve ("attack that position"). A unit with
//!   sub-units breaks its task into tasks for them (see `systems::command`).
//! - an `Order` says *how* (move here, fire there). A unit without sub-units
//!   carries out its task by issuing orders to itself.
//!
//! Both sides use exactly the same components; only `Side` and `Controller`
//! differ.

use bevy_ecs::component::Component;
use serde::{Deserialize, Serialize};

/// Which side a soldier or unit fights for. Blue is the player's side.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Side {
    #[default]
    Blue,
    Red,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Red => "red",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "blue" => Some(Self::Blue),
            "red" => Some(Self::Red),
            _ => None,
        }
    }
    pub fn enemy(self) -> Self {
        match self {
            Self::Blue => Self::Red,
            Self::Red => Self::Blue,
        }
    }
}

/// Who decides what a unit does. Orders from any other source are rejected.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Controller {
    #[default]
    Player,
    Ai,
    Script,
}

impl Controller {
    pub fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Ai => "ai",
            Self::Script => "script",
        }
    }
}

/// What a unit has been tasked to achieve. Positions are world metres; facing
/// is a yaw in radians (0 = looking down -Z, "north").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TaskKind {
    /// Take a position. A platoon picks a playbook; a section assaults it.
    Attack { objective: [f32; 3] },
    /// Hold a position facing a direction.
    Defend { position: [f32; 3], facing: f32 },
    /// Go somewhere and wait (form-up point, reserve position).
    MoveTo { position: [f32; 3], facing: f32 },
    /// Go to `position` and keep firing at `target` (suppress it).
    SupportByFire {
        position: [f32; 3],
        target: [f32; 3],
    },
}

impl TaskKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Attack { .. } => "attack",
            Self::Defend { .. } => "defend",
            Self::MoveTo { .. } => "move",
            Self::SupportByFire { .. } => "support by fire",
        }
    }
    /// The point the task is about (for maps and UI).
    pub fn point(&self) -> [f32; 3] {
        match *self {
            Self::Attack { objective } => objective,
            Self::Defend { position, .. }
            | Self::MoveTo { position, .. }
            | Self::SupportByFire { position, .. } => position,
        }
    }
}

/// How far a unit has got with its task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TaskStage {
    /// Just given; nothing done yet.
    #[default]
    New,
    /// Walking to the task's position.
    Moving,
    /// In position (and holding there).
    InPosition,
    /// In position and firing.
    Firing,
    /// Finished. Whoever gave the task decides what comes next.
    Done,
}

impl TaskStage {
    pub fn name(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Moving => "moving",
            Self::InPosition => "in position",
            Self::Firing => "firing",
            Self::Done => "done",
        }
    }
}

/// The task a unit is carrying out (None = no task).
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Task(pub Option<(TaskKind, TaskStage)>);

impl Task {
    pub fn new(kind: TaskKind) -> Self {
        Self(Some((kind, TaskStage::New)))
    }
    pub fn kind(&self) -> Option<TaskKind> {
        self.0.map(|(k, _)| k)
    }
    pub fn stage(&self) -> Option<TaskStage> {
        self.0.map(|(_, s)| s)
    }
    pub fn set_stage(&mut self, stage: TaskStage) {
        if let Some((_, s)) = &mut self.0 {
            *s = stage;
        }
    }
}

/// Who an order came from. Checked against the unit's `Controller`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Issuer {
    Player,
    Ai,
    Script,
}

impl Issuer {
    pub fn may_command(self, controller: Controller) -> bool {
        matches!(
            (self, controller),
            (Self::Player, Controller::Player)
                | (Self::Ai, Controller::Ai)
                | (Self::Script, Controller::Script)
        )
    }
}

/// How a commander has decided to carry out an attack or defence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Playbook {
    /// One section suppresses from a fire base while the others assault from
    /// a flank.
    FireBaseAssault,
    /// Two sections advance abreast onto the objective; one follows in reserve.
    TwoUp,
    /// Sections spread across a frontage, facing the threat.
    Defend,
}

impl Playbook {
    pub fn name(self) -> &'static str {
        match self {
            Self::FireBaseAssault => "fire base + assault",
            Self::TwoUp => "two up",
            Self::Defend => "defend",
        }
    }
}

/// Where a commander's plan has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanPhase {
    /// Sub-units moving into place (fire base, form-up point).
    Prepare,
    /// Sub-units assaulting the objective.
    Assault,
    /// Objective taken: sub-units hold around it.
    Consolidate,
}

/// A commander's plan for their unit's task: which sub-unit does what. Sub-units
/// are stored by unit id so the plan saves as plain data.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub playbook: Playbook,
    pub phase: PlanPhase,
    pub fire_base: Option<u32>,
    pub assault: Vec<u32>,
    pub reserve: Option<u32>,
    /// The unit's own headquarters element, if it has one.
    pub hq: Option<u32>,
    /// Direction of the attack or defence (x, z), fixed when the plan is made.
    pub axis: [f32; 2],
}
