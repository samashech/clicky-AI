//! Native Windows/X11 geometry is physical pixels; Hyprland uses logical pixels
//! end-to-end with grim scale 1 and layer-shell (never mixed with native transforms).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Bounds {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite())
            && self.width > 0.
            && self.height > 0.
    }
    pub fn contains(self, x: f64, y: f64) -> bool {
        self.valid()
            && x >= self.x
            && y >= self.y
            && x < self.x + self.width
            && y < self.y + self.height
    }
    pub fn logical(self, origin: (f64, f64), scale: f64) -> Self {
        Self {
            x: (self.x - origin.0) / scale,
            y: (self.y - origin.1) / scale,
            width: self.width / scale,
            height: self.height / scale,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UIElement {
    pub id: String,
    pub role: String,
    pub name: String,
    pub bounds: Bounds,
    pub confidence: f64,
    pub source: String,
    pub actionable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Created,
    Looking,
    Guiding,
    Waiting,
    Verifying,
    Complete,
    Uncertain,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    pub id: usize,
    pub instruction: String,
    pub target_hint: String,
    pub target: Option<UIElement>,
    pub expected_action: String,
    pub verification_condition: String,
    pub retries: u8,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub mode: String,
    pub steps: Vec<Step>,
    pub current: usize,
    pub status: Status,
    pub message: String,
    pub needs_plan: bool,
}
impl Task {
    pub fn new(id: u64, goal: &str, mode: &str) -> Result<Self, String> {
        if goal.trim().is_empty() || goal.len() > 2000 {
            return Err("Enter a request between 1 and 2000 bytes".into());
        }
        let instructions: Vec<_> = goal
            .split(" then ")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if instructions.is_empty() {
            return Err("Enter at least one instruction".into());
        }
        if instructions.len() > 20 {
            return Err("A task may contain at most 20 steps".into());
        }
        Ok(Self {
            id,
            mode: mode.into(),
            steps: instructions
                .iter()
                .enumerate()
                .map(|(id, s)| Step {
                    id,
                    instruction: s.to_string(),
                    target_hint: target_name(s),
                    target: None,
                    expected_action: "click".into(),
                    verification_condition: "pointer_inside_current_target".into(),
                    retries: 0,
                    status: Status::Created,
                })
                .collect(),
            current: 0,
            status: Status::Created,
            message: String::new(),
            needs_plan: !is_direct_request(goal),
        })
    }
    pub fn apply_plan(&mut self, steps: Vec<crate::provider::PlannedStep>) -> Result<(), String> {
        if self.current != 0 || steps.is_empty() || steps.len() > 10 {
            return Err("Invalid task plan".into());
        }
        self.steps = steps
            .into_iter()
            .enumerate()
            .map(|(id, s)| Step {
                id,
                instruction: s.instruction,
                target_hint: s.target_label,
                target: None,
                expected_action: "click".into(),
                verification_condition: "pointer_inside_current_target".into(),
                retries: 0,
                status: Status::Created,
            })
            .collect();
        self.needs_plan = false;
        Ok(())
    }
    pub fn select(&mut self, target: UIElement) -> Result<(), String> {
        if !target.bounds.valid()
            || !target.confidence.is_finite()
            || !(0.0..=1.0).contains(&target.confidence)
        {
            return Err("Invalid target geometry or confidence".into());
        }
        if self.current >= self.steps.len() || self.status == Status::Cancelled {
            return Err("Task is no longer active".into());
        }
        let s = &mut self.steps[self.current];
        s.instruction = format!(
            "Click {}.",
            target.name.chars().take(160).collect::<String>()
        );
        s.target = Some(target);
        s.status = Status::Waiting;
        self.status = Status::Waiting;
        Ok(())
    }
    /// Explicit user attestation, deliberately distinct from observed pointer hits.
    pub fn confirm(&mut self, step_id: usize) -> bool {
        if self.status != Status::Waiting
            || self.steps.get(self.current).is_none_or(|s| s.id != step_id)
        {
            return false;
        }
        let step = &mut self.steps[self.current];
        step.verification_condition = "user_confirmation".into();
        step.status = Status::Complete;
        self.current += 1;
        self.status = if self.current == self.steps.len() {
            Status::Complete
        } else {
            Status::Looking
        };
        self.message =
            "Action confirmed by user; no global click or application outcome was observed.".into();
        true
    }
    pub fn click(&mut self, x: f64, y: f64) -> bool {
        if self.status != Status::Waiting {
            return false;
        }
        let s = &mut self.steps[self.current];
        if !s.target.as_ref().is_some_and(|t| t.bounds.contains(x, y)) {
            return false;
        }
        s.status = Status::Complete;
        self.current += 1;
        self.status = if self.current == self.steps.len() {
            Status::Complete
        } else {
            Status::Looking
        };
        self.message =
            "Click location verified; application outcome is not semantically verified.".into();
        true
    }
}
pub fn is_direct_request(goal: &str) -> bool {
    let lower = goal.trim().to_lowercase();
    ![
        "how ",
        "help me create ",
        "help me make ",
        "help me crop ",
        "teach ",
        "show me how ",
    ]
    .iter()
    .any(|p| lower.starts_with(p))
}
pub fn normalize_voice_goal(goal: &str) -> String {
    let mut text = goal.trim().to_lowercase();
    for prefix in ["hey clicky ai", "hey clicky", "clicky ai"] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim_start_matches([',', ' ', '.', ':']).to_string();
            break;
        }
    }
    text
}
pub fn target_name(instruction: &str) -> String {
    let mut s = instruction
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .to_lowercase();
    for prefix in [
        "please ",
        "help me ",
        "show me ",
        "where is ",
        "where’s ",
        "find ",
        "click ",
        "open ",
        "select ",
        "the ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
        }
    }
    for suffix in [" button", " menu"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.to_string();
            break;
        }
    }
    s
}
pub fn exact_match<'a>(instruction: &str, elements: &'a [UIElement]) -> Option<&'a UIElement> {
    let literal = instruction.trim().to_lowercase();
    let exact: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.name.trim().to_lowercase() == literal && e.bounds.valid() && e.confidence >= 0.5
        })
        .collect();
    if !exact.is_empty() {
        return if exact.len() == 1 {
            Some(exact[0])
        } else {
            None
        };
    }
    let name = target_name(instruction);
    let matches: Vec<_> = elements
        .iter()
        .filter(|e| e.name.trim().to_lowercase() == name && e.bounds.valid() && e.confidence >= 0.5)
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn element() -> UIElement {
        UIElement {
            id: "1".into(),
            role: "button".into(),
            name: "File".into(),
            bounds: Bounds {
                x: -100.,
                y: 20.,
                width: 80.,
                height: 30.,
            },
            confidence: 1.,
            source: "uia".into(),
            actionable: true,
        }
    }
    #[test]
    fn confirmation_is_explicit_and_rejects_stale_steps() {
        let mut t = Task::new(1, "Click File then click Open", "guide").unwrap();
        assert!(!t.confirm(0));
        t.status = Status::Waiting;
        let id = t.steps[0].id;
        assert!(!t.confirm(id + 1));
        assert!(t.confirm(id));
        assert_eq!(t.steps[0].verification_condition, "user_confirmation");
        assert!(!t.confirm(id));
    }
    #[test]
    fn clicks_only_advance_inside_target_once() {
        let mut t = Task::new(1, "Click File then click Save", "guide").unwrap();
        t.select(element()).unwrap();
        assert!(!t.click(0., 0.));
        assert!(t.click(-90., 30.));
        assert!(!t.click(-90., 30.));
        assert_eq!(t.current, 1);
        t.select(element()).unwrap();
        assert!(t.click(-90., 30.));
        assert_eq!(t.status, Status::Complete);
    }
    #[test]
    fn mixed_dpi_negative_origin() {
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            let r = element().bounds.logical((-200., 0.), scale);
            assert_eq!(r.x, 100. / scale);
            assert_eq!(r.width, 80. / scale);
        }
    }
    #[test]
    fn ambiguous_match_requires_reasoning() {
        let e = element();
        assert!(exact_match("Click File", &[e.clone()]).is_some());
        assert!(exact_match("Click File", &[e.clone(), e]).is_none());
    }
    #[test]
    fn invalid_and_edges() {
        let r = element().bounds;
        assert!(!r.contains(-20., 25.));
        assert!(!r.contains(f64::NAN, 25.));
        assert!(!Bounds { width: 0., ..r }.valid());
        assert!(Task::new(1, "", "guide").is_err());
    }
    #[test]
    fn voice_and_visuals_share_grounded_instruction() {
        let mut task = Task::new(1, "Where is the File menu?", "guide").unwrap();
        task.select(element()).unwrap();
        assert_eq!(task.steps[0].instruction, "Click File.");
        assert_eq!(task.steps[0].target.as_ref().unwrap().name, "File");
    }
    #[test]
    fn wake_phrase_and_direct_lookup_need_no_model() {
        assert_eq!(
            target_name(&normalize_voice_goal(
                "Hey Clicky AI, help me find the export button."
            )),
            "export"
        );
        assert!(
            !Task::new(1, "where is the export button", "guide")
                .unwrap()
                .needs_plan
        );
        assert!(
            Task::new(1, "how do I create a project", "guide")
                .unwrap()
                .needs_plan
        );
    }
}
