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
    /// No orders: stay here with all-round security. This is what an AI unit
    /// does when it has nothing else to do.
    Hold { position: [f32; 3] },
    /// Take a share of a perimeter: spread along an arc round `centre`, facing
    /// outward. The arc starts at yaw `start` and runs `sweep` radians
    /// clockwise.
    Perimeter {
        centre: [f32; 3],
        radius: f32,
        start: f32,
        sweep: f32,
    },
    /// Walk a route of waypoints in order, then start again from the first.
    /// `leg` is the waypoint being walked to.
    Patrol {
        route: Route,
        #[serde(default)]
        leg: u8,
    },
    /// Fall back to `position`, then defend it facing `facing` (toward the
    /// threat).
    Withdraw { position: [f32; 3], facing: f32 },
}

/// Most waypoints a route can have.
pub const MAX_ROUTE: usize = 8;

/// A patrol route: up to `MAX_ROUTE` waypoints. Written in data files as a
/// plain list of points, e.g. `[[0, 0, -400], [200, 0, -300]]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<[f32; 3]>", into = "Vec<[f32; 3]>")]
pub struct Route {
    points: [[f32; 3]; MAX_ROUTE],
    len: u8,
}

impl Route {
    /// A route through `points` (extra points past `MAX_ROUTE` are dropped).
    pub fn new(points: &[[f32; 3]]) -> Self {
        let mut route = Self {
            points: [[0.0; 3]; MAX_ROUTE],
            len: points.len().min(MAX_ROUTE) as u8,
        };
        for (slot, p) in route.points.iter_mut().zip(points) {
            *slot = *p;
        }
        route
    }
    pub fn points(&self) -> &[[f32; 3]] {
        &self.points[..self.len as usize]
    }
    pub fn len(&self) -> usize {
        self.len as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Waypoint `i`, wrapping round to the start.
    pub fn get(&self, i: usize) -> [f32; 3] {
        if self.is_empty() {
            [0.0; 3]
        } else {
            self.points[i % self.len()]
        }
    }
}

impl From<Vec<[f32; 3]>> for Route {
    fn from(points: Vec<[f32; 3]>) -> Self {
        Self::new(&points)
    }
}

impl From<Route> for Vec<[f32; 3]> {
    fn from(route: Route) -> Self {
        route.points().to_vec()
    }
}

impl TaskKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Attack { .. } => "attack",
            Self::Defend { .. } => "defend",
            Self::MoveTo { .. } => "move",
            Self::SupportByFire { .. } => "support by fire",
            Self::Hold { .. } => "hold",
            Self::Perimeter { .. } => "perimeter",
            Self::Patrol { .. } => "patrol",
            Self::Withdraw { .. } => "withdraw",
        }
    }
    /// The point the task is about (for maps and UI).
    pub fn point(&self) -> [f32; 3] {
        match *self {
            Self::Attack { objective } => objective,
            Self::Defend { position, .. }
            | Self::MoveTo { position, .. }
            | Self::SupportByFire { position, .. }
            | Self::Hold { position }
            | Self::Withdraw { position, .. } => position,
            Self::Perimeter { centre, .. } => centre,
            Self::Patrol { route, leg } => route.get(leg as usize),
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

/// How a commander has decided to carry out their task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Playbook {
    // Attack
    /// One section suppresses from a fire base while the others assault from
    /// a flank.
    FireBaseAssault,
    /// Two sections advance abreast onto the objective; one follows in reserve.
    TwoUp,
    /// Two sections leapfrog toward the objective, one moving while the other
    /// watches, then assault together.
    BoundingAdvance,
    // Defend
    /// Sections spread across a frontage, facing the threat. (Saved as
    /// "Defend" from before there were other defences.)
    Defend,
    /// Sections share a perimeter round the position, facing every way.
    AllRoundDefence,
    // Other tasks
    /// Idle: all-round security where the unit stands.
    Harbour,
    /// Walk a route as a group, again and again.
    Patrol,
    /// Bound back to a rear position, then defend it.
    Withdraw,
    /// Every sub-unit is simply given the same task.
    Direct,
}

impl Playbook {
    pub fn name(self) -> &'static str {
        match self {
            Self::FireBaseAssault => "fire base + assault",
            Self::TwoUp => "two up",
            Self::BoundingAdvance => "bounding advance",
            Self::Defend => "linear defence",
            Self::AllRoundDefence => "all-round defence",
            Self::Harbour => "harbour",
            Self::Patrol => "patrol",
            Self::Withdraw => "withdraw",
            Self::Direct => "direct",
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
    // Added after plans first shipped (defaults keep older saves loading):
    /// Where the plan started (for bounding: distances are measured from here).
    #[serde(default)]
    pub start: [f32; 3],
    /// Metres covered along `axis` so far (bounding).
    #[serde(default)]
    pub progress: f32,
    /// The sub-unit that made the last bound (the next bound is someone else's).
    #[serde(default)]
    pub last_bounder: Option<u32>,
    /// Waypoint the unit is heading for (patrols).
    #[serde(default)]
    pub leg: u32,
}

impl Plan {
    /// A plan with no sub-units assigned yet.
    pub fn new(playbook: Playbook, phase: PlanPhase, hq: Option<u32>, axis: [f32; 2]) -> Self {
        Self {
            playbook,
            phase,
            fire_base: None,
            assault: Vec::new(),
            reserve: None,
            hq,
            axis,
            start: [0.0; 3],
            progress: 0.0,
            last_bounder: None,
            leg: 0,
        }
    }
}
