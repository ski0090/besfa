//! The Agent window's Organization tab: agents with roles, wired like a
//! Blueprint graph. A link hands one agent's result to the next, and a task
//! sent to the organization from the chat runs every agent once, in link
//! order, each answering in the chat. The roles to start from are Claude
//! Code Game Studios' (CCGS) agents.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use eframe::egui::{
    self, Align2, Color32, FontId, LayerId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, emath::TSTransform, epaint::CubicBezierShape,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    agent::{self, AgentSettings, Agents, Ask, Message, Provider},
    i18n::t,
    panels,
    process::{CliProcess, Output},
};

/// Where a project keeps its organizations, one file each, named after it.
pub const ORGANIZATIONS_DIR: &str = "besfa/organizations";

/// Next to that folder: the last run, while it has agents left to go.
const RUN_FILE: &str = "organization-run.ron";

/// Characters a Windows file name cannot hold.
const NOT_IN_NAMES: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

const NODE_SIZE: Vec2 = Vec2::new(210.0, 118.0);
const HEADER: f32 = 26.0;
const PIN: f32 = 7.0;
const GROUP_TITLE: f32 = 24.0;

/// How hard an agent thinks before it answers.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Effort {
    /// The CLI's own setting.
    #[default]
    Default,
    Low,
    Medium,
    High,
    Max,
}

impl Effort {
    const ALL: [Effort; 5] = [
        Effort::Default,
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Max,
    ];

    fn label(self) -> &'static str {
        t(match self {
            Effort::Default => "CLI default",
            Effort::Low => "Low",
            Effort::Medium => "Medium",
            Effort::High => "High",
            Effort::Max => "Max",
        })
    }

    /// The flags that ask `provider` for this effort. Gemini's CLI has none.
    fn args(self, provider: Provider) -> Vec<String> {
        let level = match self {
            Effort::Default => return Vec::new(),
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Max => "max",
        };
        match provider {
            Provider::Claude => vec!["--effort".into(), level.into()],
            Provider::OpenAi => {
                let level = if self == Effort::Max { "xhigh" } else { level };
                vec!["-c".into(), format!("model_reasoning_effort=\"{level}\"")]
            }
            Provider::Gemini => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Department {
    Leadership,
    Design,
    Programming,
    ArtAudio,
    Quality,
    Operations,
}

impl Department {
    const ALL: [Department; 6] = [
        Department::Leadership,
        Department::Design,
        Department::Programming,
        Department::ArtAudio,
        Department::Quality,
        Department::Operations,
    ];

    fn label(self) -> &'static str {
        t(match self {
            Department::Leadership => "Leadership",
            Department::Design => "Design",
            Department::Programming => "Programming",
            Department::ArtAudio => "Art & Audio",
            Department::Quality => "Quality",
            Department::Operations => "Operations",
        })
    }

    fn color(self) -> Color32 {
        match self {
            Department::Leadership => Color32::from_rgb(0xA8, 0x55, 0x55),
            Department::Design => Color32::from_rgb(0x4F, 0x86, 0x5A),
            Department::Programming => Color32::from_rgb(0x46, 0x6E, 0xA8),
            Department::ArtAudio => Color32::from_rgb(0x8A, 0x5B, 0xA6),
            Department::Quality => Color32::from_rgb(0xA8, 0x86, 0x3E),
            Department::Operations => Color32::from_rgb(0x3E, 0x8E, 0x92),
        }
    }
}

/// A CCGS agent: its file name in `.claude/agents/`, its department and
/// what it does. Its model comes from its tier in `CCGS_CHART`.
struct Role {
    name: &'static str,
    department: Department,
    summary: &'static str,
}

const fn role(name: &'static str, department: Department, summary: &'static str) -> Role {
    Role {
        name,
        department,
        summary,
    }
}

/// The Claude model and effort a CCGS role starts with, by its tier in
/// `CCGS_CHART`: directors decide, leads plan, specialists do the work.
fn tier(role: &str) -> (&'static str, Effort) {
    if CCGS_CHART.iter().any(|&(director, _, _)| director == role) {
        ("opus", Effort::High)
    } else if CCGS_CHART.iter().any(|&(_, lead, _)| lead == role) {
        ("opus", Effort::Medium)
    } else {
        ("sonnet", Effort::Default)
    }
}

/// What a CCGS role started with before the tiers (organization files at
/// version 0): CCGS's own models, opus and high effort for Leadership.
fn former_tier(role: &Role) -> (&'static str, Effort) {
    match role.name {
        _ if role.department == Department::Leadership => ("opus", Effort::High),
        "lead-programmer" | "prototyper" | "devops-engineer" | "community-manager" => {
            ("sonnet", Effort::Default)
        }
        _ => ("", Effort::Default),
    }
}

/// CCGS's agents, less the Godot, Unity and Unreal specialists, which have
/// nothing to do in a Bevy game.
const ROLES: &[Role] = {
    use Department::*;
    &[
        role(
            "creative-director",
            Leadership,
            "Highest creative authority: vision, tone and aesthetic direction; settles deadlocks between department leads.",
        ),
        role(
            "technical-director",
            Leadership,
            "High-level technical decisions: engine architecture, technology choices, performance strategy, technical risk.",
        ),
        role(
            "producer",
            Leadership,
            "Production: sprint planning, milestones, risk, scope, and coordination across departments.",
        ),
        role(
            "game-designer",
            Design,
            "Mechanics and systems design: core loops, progression, combat, economy, player-facing rules.",
        ),
        role(
            "systems-designer",
            Design,
            "Detailed subsystem mechanics: combat formulas, progression curves, crafting recipes, status effects.",
        ),
        role(
            "level-designer",
            Design,
            "Spatial design of levels: encounter layouts, pacing, difficulty, environmental storytelling.",
        ),
        role(
            "economy-designer",
            Design,
            "Resource economies: loot tables, sinks and faucets, progression curve calibration, balance.",
        ),
        role(
            "narrative-director",
            Design,
            "Story architecture, world-building, character design and dialogue strategy.",
        ),
        role(
            "writer",
            Design,
            "Player-facing text: dialogue, lore entries, item and ability descriptions.",
        ),
        role(
            "world-builder",
            Design,
            "Lore: factions, cultures, history, geography, ecology and world rules.",
        ),
        role(
            "ux-designer",
            Design,
            "User flows, interaction design, information architecture and onboarding.",
        ),
        role(
            "live-ops-designer",
            Design,
            "Post-launch content: seasonal events, battle passes, content cadence, retention.",
        ),
        role(
            "lead-programmer",
            Programming,
            "Code architecture, coding standards, code review, API design; assigns work to the programmers.",
        ),
        role(
            "gameplay-programmer",
            Programming,
            "Implements designed mechanics as code: player systems, combat, interactive features.",
        ),
        role(
            "engine-programmer",
            Programming,
            "Core engine systems: rendering pipeline, physics, memory, resource loading, scenes.",
        ),
        role(
            "ai-programmer",
            Programming,
            "Game AI: behavior trees, state machines, pathfinding, perception, NPC and enemy behavior.",
        ),
        role(
            "network-programmer",
            Programming,
            "Multiplayer: state replication, lag compensation, matchmaking, netcode and bandwidth.",
        ),
        role(
            "ui-programmer",
            Programming,
            "User interface systems: menus, HUDs, inventory screens, dialogue boxes, screen flow.",
        ),
        role(
            "tools-programmer",
            Programming,
            "Internal tools: editor extensions, content authoring tools, debug utilities, pipelines.",
        ),
        role(
            "bevy-specialist",
            Programming,
            "Bevy patterns and APIs: ECS, plugins and schedules, messages and observers, rendering, bevy_ui.",
        ),
        role(
            "prototyper",
            Programming,
            "Throwaway builds that test whether an idea is fun. Speed over standards.",
        ),
        role(
            "art-director",
            ArtAudio,
            "Visual identity: style guides, art bible, asset standards, color palettes, UI look.",
        ),
        role(
            "technical-artist",
            ArtAudio,
            "Shaders, VFX, rendering optimization and the art pipeline between art and engineering.",
        ),
        role(
            "audio-director",
            ArtAudio,
            "Sonic identity: music direction, sound palette, audio implementation, mix balance.",
        ),
        role(
            "sound-designer",
            ArtAudio,
            "SFX specifications: audio events, mixing parameters, sound categories.",
        ),
        role(
            "qa-lead",
            Quality,
            "Test strategy: test plans, bug severity, regression planning, release quality gates.",
        ),
        role(
            "qa-tester",
            Quality,
            "Writes test cases, bug reports and regression checklists.",
        ),
        role(
            "performance-analyst",
            Quality,
            "Profiling, bottlenecks, memory and frame time; recommends optimizations.",
        ),
        role(
            "accessibility-specialist",
            Quality,
            "Remapping, text scaling, colorblind modes, screen reader support.",
        ),
        role(
            "security-engineer",
            Quality,
            "Anti-cheat, exploits, secure saves and network traffic, player privacy.",
        ),
        role(
            "release-manager",
            Operations,
            "Release pipeline: certification, store submissions, version numbers, release day.",
        ),
        role(
            "devops-engineer",
            Operations,
            "Build pipelines, CI/CD, version control workflow and test automation.",
        ),
        role(
            "analytics-engineer",
            Operations,
            "Telemetry: event tracking, player behavior, A/B tests, dashboards.",
        ),
        role(
            "localization-lead",
            Operations,
            "Internationalization: string extraction, translation pipeline, locale testing.",
        ),
        role(
            "community-manager",
            Operations,
            "Patch notes, social posts, community updates, player feedback and bug triage.",
        ),
    ]
};

fn find_role(name: &str) -> Option<&'static Role> {
    ROLES.iter().find(|role| role.name == name)
}

/// A role's name as people write it: `qa-lead` is "Qa Lead".
fn title(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a line an agent's CLI printed adds to its message.
#[derive(Debug, PartialEq)]
pub enum Streamed {
    Thought(String),
    /// Part of the answer.
    Text(String),
    /// The whole answer.
    Answer(String),
    Nothing,
}

/// Reads a line of Claude's `--output-format stream-json`: what it says and
/// the tools it calls on the way are thoughts, and the result is the answer.
/// Its thinking arrives empty unless the CLI is set to show it.
pub fn streamed(line: &str) -> Streamed {
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        // A warning or error printed as it is.
        return Streamed::Text(line.into());
    };
    match event["type"].as_str() {
        Some("assistant") => {
            let blocks = event["message"]["content"].as_array();
            let said: Vec<String> = blocks
                .into_iter()
                .flatten()
                .filter_map(|block| match block["type"].as_str()? {
                    "text" => block["text"].as_str().map(str::to_string),
                    "thinking" => block["thinking"].as_str().map(str::to_string),
                    "tool_use" => {
                        // Bash's command, Read's path, Glob's pattern...
                        let input = block["input"].as_object();
                        let what = input
                            .and_then(|input| input.values().find_map(Value::as_str))
                            .and_then(|what| what.lines().next())
                            .unwrap_or_default();
                        let what: String = what.chars().take(120).collect();
                        Some(format!("[{}] {what}", block["name"].as_str()?))
                    }
                    _ => None,
                })
                .filter(|text| !text.trim().is_empty())
                .collect();
            if said.is_empty() {
                Streamed::Nothing
            } else {
                Streamed::Thought(said.join("\n"))
            }
        }
        Some("result") => Streamed::Answer(event["result"].as_str().unwrap_or_default().into()),
        _ => Streamed::Nothing,
    }
}

/// Adds `line` to what an agent said, `output`, or to what it thought and
/// did on the way.
pub fn take(line: Streamed, output: &mut String, thoughts: &mut String) {
    match line {
        Streamed::Thought(text) => push_line(thoughts, &text),
        Streamed::Text(text) => push_line(output, &text),
        // An error result can come without one; what it printed stays.
        Streamed::Answer(text) if !text.trim().is_empty() => {
            // The answer is also the last thing it said on the way.
            if let Some(rest) = thoughts.trim_end().strip_suffix(text.trim()) {
                *thoughts = rest.trim_end().into();
            }
            *output = text;
        }
        Streamed::Answer(_) | Streamed::Nothing => {}
    }
}

fn push_line(text: &mut String, line: &str) {
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(line);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum NodeState {
    #[default]
    Idle,
    Waiting,
    Running,
    Done,
    Failed,
}

/// One agent of the organization.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Node {
    id: u32,
    name: String,
    /// The CCGS agent it plays, or empty for one of the user's own.
    role: String,
    instructions: String,
    provider: Provider,
    /// Empty for the CLI's default.
    model: String,
    effort: Effort,
    pos: [f32; 2],
    /// What it answered on the last run.
    #[serde(skip)]
    output: String,
    /// What it thought and did on the way to that answer.
    #[serde(skip)]
    thoughts: String,
    /// What it answered the round before, when it runs again.
    #[serde(skip)]
    last: String,
    #[serde(skip)]
    state: NodeState,
}

impl Node {
    /// Takes a line its CLI printed: Claude's are stream-json events, Codex
    /// logs its run and leaves the answer in a file, Gemini prints the
    /// answer.
    fn take(&mut self, line: &str) {
        let line = match self.provider {
            Provider::Claude => streamed(line),
            Provider::OpenAi => Streamed::Thought(line.into()),
            Provider::Gemini => Streamed::Text(line.into()),
        };
        take(line, &mut self.output, &mut self.thoughts);
    }

    fn color(&self) -> Color32 {
        find_role(&self.role).map_or(Color32::from_gray(0x5A), |role| role.department.color())
    }

    fn rect(&self, origin: Vec2) -> Rect {
        Rect::from_min_size(Pos2::from(self.pos) + origin, NODE_SIZE)
    }
}

/// A box around nodes to keep them together, like a Blueprint comment.
/// Moving it moves the nodes inside.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Group {
    id: u32,
    name: String,
    /// Its corners, in canvas space.
    min: [f32; 2],
    max: [f32; 2],
    color: [u8; 3],
}

impl Default for Group {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Group".into(),
            min: [0.0; 2],
            max: [300.0, 200.0],
            color: [0x6E, 0x7C, 0x91],
        }
    }
}

impl Group {
    fn rect(&self, origin: Vec2) -> Rect {
        Rect::from_min_max(Pos2::from(self.min) + origin, Pos2::from(self.max) + origin)
    }
}

/// The agents, which hands each result to whom, and the groups.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Organization {
    nodes: Vec<Node>,
    /// From the agent whose result goes, to the agent it goes to.
    links: Vec<(u32, u32)>,
    groups: Vec<Group>,
    next_id: u32,
    /// 0 in files from before roles started with their tier's model; reading
    /// one moves its agents still on CCGS's own models to their tier's.
    version: u32,
}

impl Organization {
    /// What `file` keeps, or an empty organization.
    pub fn read(file: &Path) -> Self {
        read_ron(file)
            .map(Self::tidy)
            .map(Self::upgrade)
            .unwrap_or_default()
    }

    fn upgrade(mut self) -> Self {
        if self.version == 0 {
            for node in &mut self.nodes {
                let Some(role) = find_role(&node.role) else {
                    continue;
                };
                let (model, effort) = former_tier(role);
                if node.provider == Provider::Claude && node.model == model && node.effort == effort
                {
                    let (model, effort) = tier(role.name);
                    node.model = model.into();
                    node.effort = effort;
                }
            }
        }
        self.version = 1;
        self
    }

    /// Without links the editor cannot make, which a hand-edited or merged
    /// file can have: to agents that are gone, twice, or into a loop.
    /// Ids are kept unique too, and `next_id` past every one in use.
    fn tidy(mut self) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        self.nodes.retain(|node| seen.insert(node.id));
        self.groups.retain(|group| seen.insert(group.id));
        self.next_id = seen.into_iter().fold(self.next_id, u32::max);
        let links = std::mem::take(&mut self.links);
        for (from, to) in links {
            if self.node(from).is_some() && self.node(to).is_some() {
                self.link(from, to);
            }
        }
        self
    }

    fn write(&self, file: &Path) -> Result<(), String> {
        write_ron(
            file,
            &Self {
                version: 1,
                ..self.clone()
            },
        )
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    /// Adds an agent at `pos` playing the CCGS role `name`, or one of the
    /// user's own when `name` is empty.
    fn add(&mut self, name: &str, pos: Pos2) -> u32 {
        let id = self.id();
        let role = find_role(name);
        let (model, effort) = role.map_or(("", Effort::Default), |role| tier(role.name));
        self.nodes.push(Node {
            id,
            name: role.map_or_else(|| "Agent".into(), |role| title(role.name)),
            role: name.into(),
            instructions: role.map_or_else(String::new, |role| role.summary.into()),
            model: model.into(),
            effort,
            pos: pos.into(),
            ..Default::default()
        });
        id
    }

    fn remove(&mut self, id: u32) {
        self.nodes.retain(|node| node.id != id);
        self.links.retain(|&(from, to)| from != id && to != id);
    }

    /// Links `from` to `to` unless that would make a loop.
    fn link(&mut self, from: u32, to: u32) -> bool {
        if from == to || self.links.contains(&(from, to)) {
            return false;
        }
        self.links.push((from, to));
        if self.order().is_err() {
            self.links.pop();
            return false;
        }
        true
    }

    /// Every agent after the agents it takes results from, the earlier
    /// added first where it is free to choose, or an error on a loop.
    fn order(&self) -> Result<Vec<u32>, &'static str> {
        let mut waiting: BTreeMap<u32, usize> =
            self.nodes.iter().map(|node| (node.id, 0)).collect();
        for &(_, to) in &self.links {
            *waiting.entry(to).or_default() += 1;
        }
        let mut order = Vec::new();
        while order.len() < self.nodes.len() {
            let next = self
                .nodes
                .iter()
                .find(|node| waiting.get(&node.id) == Some(&0))
                .ok_or(t("The links make a loop."))?
                .id;
            waiting.remove(&next);
            for &(from, to) in &self.links {
                if from == next
                    && let Some(count) = waiting.get_mut(&to)
                {
                    *count -= 1;
                }
            }
            order.push(next);
        }
        Ok(order)
    }

    /// What goes to `id`'s CLI in `round`: its role, the task, what it
    /// answered the round before, and what the agents linked into it did.
    fn input(&self, id: u32, task: &str, round: u32) -> String {
        let Some(node) = self.node(id) else {
            return task.into();
        };
        let mut input = format!(
            "You are {}, one agent in a team working on this game project together.\n",
            node.name
        );
        if !node.instructions.trim().is_empty() {
            input.push_str(&format!("\nYour role:\n{}\n", node.instructions.trim()));
        }
        input.push_str(&format!("\nThe team's task:\n{}\n", task.trim()));
        if !node.last.trim().is_empty() {
            input.push_str(&format!(
                "\nThis is round {round} of at most {MAX_ROUNDS}. You said you had more to do, or an agent before you did. What you reported the round before:\n{}\n",
                node.last.trim()
            ));
            if round >= MAX_ROUNDS {
                input.push_str("\nNo round follows this one: finish your part now.\n");
            }
        }
        let earlier: Vec<&Node> = self
            .links
            .iter()
            .filter(|&&(_, to)| to == id)
            .filter_map(|&(from, _)| self.node(from))
            .collect();
        if !earlier.is_empty() {
            input.push_str("\nWhat the agents before you reported:\n");
            for node in earlier {
                input.push_str(&format!("\n## {}\n{}\n", node.name, node.output.trim()));
            }
        }
        // Role files that wait for the user's yes before writing would
        // otherwise end on a question nobody reads until the run is over.
        input.push_str(
            "\nThis run is headless: the agents go one after another and nobody reads your questions until the last one is done. The user's task is their approval for the work it asks of you, files, commits and pull requests included, so do that work now rather than asking whether you may. Where the task leaves a choice open, decide, act on it and say what you chose. Put anything only the user can settle under a heading \"For the user\" at the end of your report, and do not stop for it.\n\nDo your part of the task only. End with a short report of what you did and decided, for the agents after you, and then a last line: STATUS: DONE when your part is finished, or STATUS: MORE when you or the agents after you can do more of it once the others have reported. The agents that say MORE run again with the new reports, together with the agents after them. What only the user can settle is not MORE.",
        );
        input
    }

    /// Where a template goes: right of everything already on the canvas,
    /// so loading one never covers the user's agents.
    fn free_x(&self) -> f32 {
        let right = self
            .nodes
            .iter()
            .map(|node| node.pos[0] + NODE_SIZE.x)
            .chain(self.groups.iter().map(|group| group.max[0]))
            .fold(f32::NEG_INFINITY, f32::max);
        if right.is_finite() { right + 80.0 } else { 0.0 }
    }

    fn add_group(&mut self, name: &str, min: [f32; 2], max: [f32; 2]) {
        let id = self.id();
        self.groups.push(Group {
            id,
            name: name.into(),
            min,
            max,
            ..Default::default()
        });
    }

    /// A small studio to start from: direction, design, code and testing.
    fn starter(&mut self) {
        let x = self.free_x();
        let ids: Vec<u32> = [
            "creative-director",
            "game-designer",
            "gameplay-programmer",
            "qa-tester",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, role)| self.add(role, Pos2::new(x + 40.0 + i as f32 * 260.0, 40.0)))
        .collect();
        for pair in ids.windows(2) {
            self.link(pair[0], pair[1]);
        }
        self.add_group("Studio", [x + 20.0, 0.0], [x + 1050.0, 180.0]);
    }

    /// CCGS's studio: directors hand the task to their department leads,
    /// and leads to their specialists, one lead and its specialists a row.
    fn ccgs(&mut self) {
        let x = self.free_x();
        let (row, column) = (150.0, 260.0);
        let y = |row_index: f32| 40.0 + row_index * row;
        let mut directors: Vec<(&str, u32)> = Vec::new();
        for (director, _, _) in CCGS_CHART {
            if directors.iter().all(|&(name, _)| name != *director) {
                // Level with the middle of its leads' rows.
                let rows: Vec<f32> = (0..CCGS_CHART.len())
                    .filter(|&i| CCGS_CHART[i].0 == *director)
                    .map(|i| i as f32)
                    .collect();
                let middle = rows.iter().sum::<f32>() / rows.len() as f32;
                let id = self.add(director, Pos2::new(x + 40.0, y(middle)));
                directors.push((director, id));
            }
        }
        let leads: Vec<u32> = CCGS_CHART
            .iter()
            .enumerate()
            .map(|(i, &(director, lead, _))| {
                let id = self.add(lead, Pos2::new(x + 40.0 + column, y(i as f32)));
                let (_, from) = directors
                    .iter()
                    .find(|&&(name, _)| name == director)
                    .unwrap();
                self.link(*from, id);
                id
            })
            .collect();
        let mut widest = 0;
        for (i, (_, _, specialists)) in CCGS_CHART.iter().enumerate() {
            widest = widest.max(specialists.len());
            for (k, specialist) in specialists.iter().enumerate() {
                let pos = Pos2::new(x + 40.0 + column * (2 + k) as f32, y(i as f32));
                let id = self.add(specialist, pos);
                self.link(leads[i], id);
            }
        }
        let bottom = y(CCGS_CHART.len() as f32 - 1.0) + NODE_SIZE.y + 20.0;
        let tier = |first: usize, columns: usize| {
            let left = x + 40.0 + column * first as f32;
            [
                left - 20.0,
                left + column * (columns - 1) as f32 + NODE_SIZE.x + 20.0,
            ]
        };
        for (name, [left, right]) in [
            ("Directors", tier(0, 1)),
            ("Department leads", tier(1, 1)),
            ("Specialists", tier(2, widest)),
        ] {
            self.add_group(name, [left, 0.0], [right, bottom]);
        }
    }
}

/// CCGS's chart, a row for each department lead: the director it answers
/// to, the lead, and its specialists. The tiers also set each role's
/// model; see `tier`. shortcut: CCGS's README names the
/// three tiers but not who reports to whom, so leads take their
/// department's specialists; follow CCGS if it publishes its chart.
const CCGS_CHART: &[(&str, &str, &[&str])] = &[
    (
        "creative-director",
        "game-designer",
        &[
            "systems-designer",
            "level-designer",
            "economy-designer",
            "ux-designer",
            "live-ops-designer",
        ],
    ),
    (
        "creative-director",
        "narrative-director",
        &["writer", "world-builder"],
    ),
    ("creative-director", "art-director", &["technical-artist"]),
    ("creative-director", "audio-director", &["sound-designer"]),
    (
        "technical-director",
        "lead-programmer",
        &[
            "gameplay-programmer",
            "engine-programmer",
            "ai-programmer",
            "network-programmer",
            "ui-programmer",
            "tools-programmer",
            "bevy-specialist",
            "prototyper",
        ],
    ),
    (
        "technical-director",
        "qa-lead",
        &[
            "qa-tester",
            "performance-analyst",
            "accessibility-specialist",
            "security-engineer",
        ],
    ),
    (
        "producer",
        "release-manager",
        &["devops-engineer", "analytics-engineer", "community-manager"],
    ),
    ("producer", "localization-lead", &[]),
];

/// A program, its arguments and its environment.
type Invocation = (String, Vec<String>, Vec<(String, String)>);

/// How many times a run may go through its agents. ponytail: fixed; make
/// it a setting if three rounds are too few or cost too much.
const MAX_ROUNDS: u32 = 3;

/// Whether an agent's last status line says it has more to do. Markdown,
/// spaces and what follows the word are let go: `**Status:** MORE (QA)`
/// is MORE, and so is one with a "For the user" part after it. A quoted
/// line (`> STATUS: MORE`) is another agent's, and an answer with no
/// status line counts as done.
fn says_more(output: &str) -> bool {
    output
        .lines()
        .rev()
        .filter_map(|line| {
            let bare: String = line.chars().filter(|c| !"*`_".contains(*c)).collect();
            let bare = bare.trim_start_matches(|c: char| c.is_whitespace() || "#-".contains(c));
            let status = bare.get(..7)?;
            status
                .eq_ignore_ascii_case("STATUS:")
                .then(|| bare[7..].trim_start().to_string())
        })
        .next()
        .is_some_and(|word| {
            let word: String = word.chars().take_while(char::is_ascii_alphabetic).collect();
            word.eq_ignore_ascii_case("MORE")
        })
}

/// A task going through the organization, one agent at a time. ponytail:
/// agents on separate branches could run at once; run them in parallel if
/// waiting on them gets slow.
struct Run {
    organization: String,
    task: String,
    files: Vec<PathBuf>,
    /// The chat it answers in.
    chat: u64,
    order: Vec<u32>,
    /// Each agent's program, arguments and environment, by its place in
    /// `order`.
    invocations: Vec<Invocation>,
    next: usize,
    /// Which time through the agents this is, from 1.
    round: u32,
    process: Option<CliProcess>,
    /// Where each agent's CLI writes its final answer, when it can, by its
    /// place in `order`. Codex logs the run, prompt included, on stderr.
    answers: Vec<Option<PathBuf>>,
    stopped: bool,
    directory: PathBuf,
    /// A file the run may change, and when it was changed before the run.
    watched: (PathBuf, Option<SystemTime>),
}

/// A run that stopped before its last agent was done, kept in the project
/// so it can go on, even after the editor restarts.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Paused {
    organization: String,
    task: String,
    files: Vec<PathBuf>,
    /// Each agent that was done: its id, answer and thoughts.
    done: Vec<(u32, String, String)>,
    /// The round it was in; 0 in files from before rounds, which is 1.
    round: u32,
    /// What each agent of this round past the first answered the round
    /// before, those done in it too.
    last: Vec<(u32, String)>,
    /// The chat it answers in, and that chat's messages.
    chat: u64,
    messages: Vec<Message>,
}

impl Paused {
    pub fn organization(&self) -> &str {
        &self.organization
    }

    /// The chat it answers in, and that chat's messages.
    pub fn chat(&self) -> (u64, &[Message]) {
        (self.chat, &self.messages)
    }

    /// The files attached in that chat.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }
}

/// Where the pointer is on the graph's zoomed layer that `ui` draws on.
fn graph_pointer(ui: &Ui) -> Option<Pos2> {
    let pointer = ui.input(|input| input.pointer.latest_pos())?;
    let from_global = ui.ctx().layer_transform_from_global(ui.layer_id());
    Some(from_global.map_or(pointer, |from_global| from_global * pointer))
}

/// The Organization tab: the role palette, the graph and the picked node or
/// group. Tasks come from the chat.
pub struct OrgPanel {
    org: Organization,
    /// The folder of every organization.
    dir: PathBuf,
    /// The one shown, and the one a run goes through.
    name: String,
    /// Every organization's name, sorted.
    names: Vec<String>,
    /// The name field, which renames the organization once left.
    rename: String,
    /// The organization changed since it was last written.
    unsaved: bool,
    pan: Vec2,
    /// How big the graph is drawn, 1 for its own size.
    zoom: f32,
    selected: Vec<u32>,
    group: Option<u32>,
    /// The node a link is being dragged out of.
    wiring: Option<u32>,
    run: Option<Run>,
    /// The last run, if it stopped before its last agent was done.
    paused: Option<Paused>,
    /// Why the last link or name was refused.
    refused: Option<&'static str>,
    /// Laid-out code blocks and the like of the last result's Markdown.
    markdown: egui_commonmark::CommonMarkCache,
}

impl OrgPanel {
    /// The organizations `dir` keeps, showing the first. A project's one
    /// organization from before there could be more moves in with the
    /// name "Organization".
    pub fn new(dir: PathBuf) -> Self {
        let old = dir.parent().map(|besfa| besfa.join("organization.besfa"));
        if let Some(old) = old
            && old.is_file()
            && !dir.exists()
            && fs::create_dir_all(&dir).is_ok()
        {
            let _ = fs::rename(&old, dir.join("Organization.besfa"));
        }
        let mut names: Vec<String> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "besfa")
            })
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_string()))
            .collect();
        names.sort();
        // Written once it has something in it.
        if names.is_empty() {
            names.push(t("Organization").into());
        }
        let name = names[0].clone();
        let paused = read_ron::<Paused>(&dir.with_file_name(RUN_FILE))
            .filter(|paused| dir.join(format!("{}.besfa", paused.organization)).is_file());
        Self {
            org: Organization::read(&dir.join(format!("{name}.besfa"))),
            dir,
            rename: name.clone(),
            name,
            names,
            unsaved: false,
            pan: Vec2::ZERO,
            zoom: 1.0,
            selected: Vec::new(),
            group: None,
            wiring: None,
            run: None,
            paused,
            refused: None,
            markdown: Default::default(),
        }
    }

    /// The starter studio halfway through a task, for the manual's
    /// screenshot.
    #[cfg(test)]
    pub fn demo() -> Self {
        let mut panel = Self::new(std::env::temp_dir().join("besfa-org-demo/organizations"));
        panel.org = Organization::default();
        panel.org.starter();
        let art = panel.org.add("art-director", Pos2::new(300.0, 230.0));
        let ids: Vec<u32> = panel.org.nodes.iter().map(|node| node.id).collect();
        panel.org.link(ids[0], art);
        for (id, state) in ids
            .iter()
            .zip([NodeState::Done, NodeState::Done, NodeState::Running])
        {
            panel.org.node_mut(*id).unwrap().state = state;
        }
        panel.org.nodes[1].output = "Jumping costs 10 stamina and works once in the air.".into();
        panel.selected = vec![ids[1]];
        panel
    }

    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// The last run, if it stopped before its last agent was done.
    pub fn paused(&self) -> Option<&Paused> {
        self.paused.as_ref()
    }

    fn run_file(&self) -> PathBuf {
        self.dir.with_file_name(RUN_FILE)
    }

    /// Where the run is, with `chat`, for a resume after a restart.
    fn snapshot(&self, chat: &[Message]) -> Option<Paused> {
        let run = self.run.as_ref()?;
        let done = run
            .order
            .iter()
            .filter_map(|&id| self.org.node(id))
            .filter(|node| node.state == NodeState::Done)
            .map(|node| (node.id, node.output.clone(), node.thoughts.clone()))
            .collect();
        let last = run
            .order
            .iter()
            .filter_map(|&id| self.org.node(id))
            .filter(|node| !node.last.is_empty())
            .map(|node| (node.id, node.last.clone()))
            .collect();
        Some(Paused {
            organization: run.organization.clone(),
            task: run.task.clone(),
            files: run.files.clone(),
            done,
            round: run.round,
            last,
            chat: run.chat,
            messages: chat.to_vec(),
        })
    }

    /// Writes where the run is. ponytail: a failed write only costs the
    /// resume after a restart; tell the user if that ever bites.
    fn record(&self, chat: &[Message]) {
        if let Some(paused) = self.snapshot(chat) {
            let _ = write_ron(&self.run_file(), &paused);
        }
    }

    /// Runs the paused run's agents that were not done, handing on what
    /// the done ones answered.
    pub fn resume(
        &mut self,
        settings: &AgentSettings,
        agents: &Agents,
        directory: &Path,
        watch: PathBuf,
    ) -> Result<(), String> {
        let paused = self.paused.clone().ok_or(t("Nothing to continue."))?;
        self.begin(paused, settings, agents, directory, watch)
    }

    /// Every organization's name, for the chat to pick from.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.besfa"))
    }

    /// Whether another organization than the shown one has `name`'s file.
    /// Windows file names ignore case, so "studio" would overwrite
    /// "Studio".
    fn taken(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.names
            .iter()
            .any(|other| *other != self.name && other.to_lowercase() == lower)
    }

    /// Writes the shown organization if it changed.
    fn save(&mut self) -> Result<(), String> {
        if !self.unsaved {
            return Ok(());
        }
        self.unsaved = false;
        self.org.write(&self.file(&self.name))
    }

    /// Shows `name`'s organization in place of this one, which is saved
    /// first.
    fn open(&mut self, name: &str) -> Result<(), String> {
        self.save()?;
        self.org = Organization::read(&self.file(name));
        self.name = name.into();
        self.rename = name.into();
        self.selected.clear();
        self.group = None;
        self.wiring = None;
        self.refused = None;
        Ok(())
    }

    /// Adds an empty organization and shows it.
    fn add_organization(&mut self) -> Result<(), String> {
        let base = t("Organization");
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    base.to_string()
                } else {
                    format!("{base} {n}")
                }
            })
            .find(|name| !self.taken(name) && name.to_lowercase() != self.name.to_lowercase())
            .expect("some number is free");
        self.open(&name)?;
        self.names.push(name);
        self.names.sort();
        self.unsaved = true;
        Ok(())
    }

    /// Gives the shown organization the name in the name field.
    fn rename_to_field(&mut self) -> Result<(), String> {
        let name = self.rename.trim().to_string();
        if name == self.name {
            return Ok(());
        }
        let refused = if name.is_empty() || name.contains(NOT_IN_NAMES) {
            Some(t("A name cannot be empty or hold \\ / : * ? \" < > |"))
        } else if self.taken(&name) {
            Some(t("Another organization has that name."))
        } else {
            None
        };
        if let Some(refused) = refused {
            self.refused = Some(refused);
            self.rename = self.name.clone();
            return Ok(());
        }
        let old = self.file(&self.name);
        if old.is_file() {
            fs::rename(&old, self.file(&name)).map_err(|error| error.to_string())?;
        }
        self.names.retain(|other| *other != self.name);
        self.names.push(name.clone());
        self.names.sort();
        let old = std::mem::replace(&mut self.name, name);
        self.refused = None;
        // A paused run goes on under the new name.
        if let Some(paused) = &mut self.paused
            && paused.organization == old
        {
            paused.organization.clone_from(&self.name);
            let _ = write_ron(&self.dir.with_file_name(RUN_FILE), paused);
        }
        Ok(())
    }

    /// Deletes the shown organization's file and shows another, or a new
    /// empty one if it was the last.
    fn delete_organization(&mut self) -> Result<(), String> {
        let file = self.file(&self.name);
        if file.is_file() {
            fs::remove_file(&file).map_err(|error| error.to_string())?;
        }
        self.unsaved = false;
        self.names.retain(|other| *other != self.name);
        // Its paused run would hand its answers to another organization
        // that takes the name, whose agents can have the same ids.
        if self
            .paused
            .as_ref()
            .is_some_and(|paused| paused.organization == self.name)
        {
            self.paused = None;
            let _ = fs::remove_file(self.run_file());
        }
        match self.names.first().cloned() {
            Some(name) => self.open(&name),
            None => {
                self.names.clear();
                self.org = Organization::default();
                self.add_organization()
            }
        }
    }

    /// Runs `task`, with `files` attached, through every agent of the
    /// organization `name` in `directory`, each the way `settings` reaches
    /// its provider, answering in the chat `chat`. The tab shows that
    /// organization while it runs. `poll` tells once it is done whether it
    /// changed `watch`. Says why when it cannot start.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        name: &str,
        task: String,
        files: &[PathBuf],
        chat: u64,
        settings: &AgentSettings,
        agents: &Agents,
        directory: &Path,
        watch: PathBuf,
    ) -> Result<(), String> {
        let run = Paused {
            organization: name.into(),
            task,
            files: files.to_vec(),
            chat,
            ..Default::default()
        };
        self.begin(run, settings, agents, directory, watch)
    }

    /// Runs `run`'s organization, the agents it has done keeping their
    /// answers.
    fn begin(
        &mut self,
        run: Paused,
        settings: &AgentSettings,
        agents: &Agents,
        directory: &Path,
        watch: PathBuf,
    ) -> Result<(), String> {
        let name = run.organization.as_str();
        if !self.names.iter().any(|other| other == name) {
            return Err(t("There is no organization named \"{}\".").replace("{}", name));
        }
        self.open(name)?;
        let files = run.files.as_slice();
        let order = self.org.order()?;
        if order.is_empty() {
            return Err(
                t("The organization has no agents yet. Add them in the Organization tab.").into(),
            );
        }
        let access = settings.project(directory);
        let mut invocations = Vec::new();
        let mut answers = Vec::new();
        // Done ones too: a later round can run them again.
        for &id in &order {
            let node = self.org.node(id).expect("ordered nodes exist");
            let route = match agents.route(node.provider, settings.access(node.provider)) {
                Ok(route) => route,
                // A done one only runs again in a later round, which then
                // fails to start it and pauses with the reason.
                Err(error) if run.done.iter().any(|done| done.0 == id) => {
                    let why = format!("{} ({}): {error}", node.name, node.provider.label());
                    invocations.push((String::new(), vec![why], Vec::new()));
                    answers.push(None);
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "{} ({}): {error}",
                        node.name,
                        node.provider.label()
                    ));
                }
            };
            let (program, mut args, env) =
                agent::invocation(node.provider, &route, &node.model, files, &access);
            args.extend(node.effort.args(node.provider));
            // In a CCGS game Claude can play the role's own agent file.
            let agent_file = directory.join(format!(".claude/agents/{}.md", node.role));
            if node.provider == Provider::Claude && !node.role.is_empty() && agent_file.is_file() {
                args.extend(["--agent".into(), node.role.clone()]);
            }
            let answer = (node.provider == Provider::OpenAi).then(|| {
                std::env::temp_dir().join(format!("besfa-org-{}-{id}.txt", std::process::id()))
            });
            if let Some(answer) = &answer {
                args.extend(["--output-last-message".into(), answer.display().to_string()]);
            }
            invocations.push((program, args, env));
            answers.push(answer);
        }
        for node in &mut self.org.nodes {
            node.output.clear();
            node.thoughts.clear();
            node.state = NodeState::Waiting;
            node.last = run
                .last
                .iter()
                .find(|last| last.0 == node.id)
                .map(|last| last.1.clone())
                .unwrap_or_default();
            if let Some((_, output, thoughts)) = run.done.iter().find(|done| done.0 == node.id) {
                node.output.clone_from(output);
                node.thoughts.clone_from(thoughts);
                node.state = NodeState::Done;
            }
        }
        // A new run takes the place of a paused one.
        self.paused = None;
        self.run = Some(Run {
            organization: run.organization,
            task: run.task,
            files: run.files,
            chat: run.chat,
            order,
            invocations,
            next: 0,
            round: run.round.max(1),
            process: None,
            answers,
            stopped: false,
            directory: directory.into(),
            watched: (watch.clone(), modified(&watch)),
        });
        Ok(())
    }

    /// Stops the agent at work, and keeps the next one from starting.
    /// Sends `line` to the running agent.
    pub fn send(&mut self, line: &str) {
        if let Some(process) = self.run.as_mut().and_then(|run| run.process.as_mut()) {
            process.send(line);
        }
    }

    pub fn stop(&mut self) {
        if let Some(run) = &mut self.run {
            run.stopped = true;
            if let Some(process) = &run.process {
                process.stop();
            }
        }
    }

    /// Starts the next agent once the one before it exits, each answering
    /// in a message of its own at the end of `chat`. Once the last one is
    /// done, or one fails, returns whether the run changed the watched file.
    /// What the running agent asks the user goes to `asks`.
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        (chat, asks): (&mut Vec<Message>, &mut Vec<Ask>),
    ) -> Option<bool> {
        let run = self.run.as_mut()?;
        if run.process.is_none() && run.stopped {
            return self.finish(chat);
        }
        if run.process.is_none() {
            // Ones done before a resume answered already.
            while let Some(&id) = run.order.get(run.next)
                && self
                    .org
                    .node(id)
                    .is_some_and(|node| node.state == NodeState::Done)
            {
                run.next += 1;
            }
            let id = run.order.get(run.next).copied();
            if let Some(id) = id {
                let input = self.org.input(id, &run.task, run.round);
                let (program, args, env) = &run.invocations[run.next];
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let env: Vec<(&str, &str)> = env
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                let node = self.org.node_mut(id).expect("ordered nodes exist");
                let color = node.color();
                chat.push(Message {
                    speaker: Some((node.name.clone(), [color.r(), color.g(), color.b()])),
                    ..Default::default()
                });
                let started = if program.is_empty() {
                    // Its CLI could not be reached; the reason is the one arg.
                    Err(args.concat())
                } else {
                    CliProcess::start(&run.directory, program, &args, &env, ctx)
                        .map_err(|error| format!("Could not start {program}: {error}"))
                };
                match started {
                    Ok(mut process) => {
                        agent::send_prompt(&mut process, program, input);
                        node.state = NodeState::Running;
                        run.process = Some(process);
                    }
                    Err(error) => {
                        node.output = error;
                        node.state = NodeState::Failed;
                        show(node, chat);
                        return self.finish(chat);
                    }
                }
                // Closing the editor now kills the agent; it runs again.
                self.record(chat);
            } else if self.next_round(chat) {
                self.record(chat);
                ctx.request_repaint();
                return None;
            } else {
                return self.finish(chat);
            }
        }
        let run = self.run.as_mut()?;
        let id = run.order[run.next];
        let outputs = run.process.as_ref()?.poll();
        if outputs.is_empty() {
            return None;
        }
        let node = self.org.node_mut(id)?;
        let mut failed = false;
        for output in outputs {
            match output {
                Output::Line(line) => match Ask::read(&line) {
                    Some(ask) => asks.push(ask),
                    None => {
                        if agent::ended(&line)
                            && let Some(process) = &mut run.process
                        {
                            process.close();
                        }
                        node.take(&line);
                    }
                },
                Output::Exit(code) => {
                    run.process = None;
                    let answer = run.answers[run.next].clone();
                    let text = answer
                        .as_ref()
                        .and_then(|file| fs::read_to_string(file).ok());
                    if let Some(file) = &answer {
                        let _ = fs::remove_file(file);
                    }
                    // One that finished as Stop came keeps its answer.
                    failed = code != 0;
                    if failed {
                        let why = if run.stopped {
                            "Stopped.".to_string()
                        } else {
                            format!("Exited with code {code}.")
                        };
                        push_line(&mut node.output, &why);
                        node.state = NodeState::Failed;
                        break;
                    }
                    if let Some(text) = text {
                        node.output = text;
                    }
                    node.state = NodeState::Done;
                    run.next += 1;
                    ctx.request_repaint();
                    break;
                }
            }
        }
        let done = node.state == NodeState::Done;
        show(node, chat);
        if failed {
            return self.finish(chat);
        }
        if done {
            self.record(chat);
        }
        None
    }

    /// Goes through the agents again, unless the last round is over: those
    /// that said they have more to do, and the agents after them, which
    /// take their new answers. False when none said so.
    fn next_round(&mut self, chat: &mut Vec<Message>) -> bool {
        let Some(run) = &mut self.run else {
            return false;
        };
        let mut again: Vec<u32> = run
            .order
            .iter()
            .copied()
            .filter(|&id| {
                self.org
                    .node(id)
                    .is_some_and(|node| says_more(&node.output))
            })
            .collect();
        if again.is_empty() || run.round >= MAX_ROUNDS {
            return false;
        }
        // In run order, so one pass reaches every agent after them.
        for &id in &run.order {
            if !again.contains(&id)
                && self
                    .org
                    .links
                    .iter()
                    .any(|&(from, to)| to == id && again.contains(&from))
            {
                again.push(id);
            }
        }
        run.round += 1;
        run.next = 0;
        for &id in &again {
            let node = self.org.node_mut(id).expect("ordered nodes exist");
            node.last = std::mem::take(&mut node.output);
            node.thoughts.clear();
            node.state = NodeState::Waiting;
        }
        // Said by the organization, not the chat's own agent.
        chat.push(Message {
            speaker: Some((run.organization.clone(), [0x8A; 3])),
            text: t("Round {} of {}: {} agents go again.")
                .replacen("{}", &run.round.to_string(), 1)
                .replacen("{}", &MAX_ROUNDS.to_string(), 1)
                .replacen("{}", &again.len().to_string(), 1),
            ..Default::default()
        });
        true
    }

    /// Ends the run. One with agents left stays paused, in the project too.
    fn finish(&mut self, chat: &[Message]) -> Option<bool> {
        let paused = self.snapshot(chat)?;
        let run = self.run.take()?;
        let all_done = run.order.iter().all(|&id| {
            self.org
                .node(id)
                .is_some_and(|node| node.state == NodeState::Done)
        });
        if all_done {
            let _ = fs::remove_file(self.run_file());
        } else {
            let _ = write_ron(&self.run_file(), &paused);
            self.paused = Some(paused);
        }
        for node in &mut self.org.nodes {
            if node.state == NodeState::Waiting {
                node.state = NodeState::Idle;
            }
        }
        let (file, before) = run.watched;
        Some(modified(&file) != before)
    }

    /// Shows the tab. Returns an error if an organization could not be
    /// saved, renamed or deleted.
    pub fn ui(&mut self, ui: &mut Ui) -> Option<String> {
        let before = (self.name.clone(), self.org.clone());
        egui::Panel::left("org_roles")
            .default_size(200.0)
            .show(ui, |ui| self.palette(ui));
        if self.picked_node().is_some() || self.group.is_some() {
            egui::Panel::right("org_details")
                .default_size(260.0)
                .show(ui, |ui| self.details(ui));
        }
        let picked = egui::CentralPanel::default()
            .show(ui, |ui| {
                let picked = self.organizations(ui);
                self.toolbar(ui);
                self.canvas(ui);
                picked
            })
            .inner;
        // Outputs change while it runs and are not kept. Another one shown
        // is not a change.
        if self.name == before.0 && self.org != before.1 {
            self.unsaved = true;
        }
        if let Err(error) = picked {
            return Some(error);
        }
        // Not on every frame of a drag.
        if !ui.input(|input| input.pointer.any_down()) {
            return self.save().err();
        }
        None
    }

    fn picked_node(&self) -> Option<u32> {
        match self.selected.as_slice() {
            [id] => Some(*id),
            _ => None,
        }
    }

    /// Where a node added now goes: the middle of the view, a little lower
    /// for each node already there.
    fn drop_point(&self, ui: &Ui) -> Pos2 {
        let view = ui.ctx().content_rect().size();
        let step = (self.org.nodes.len() % 6) as f32 * 24.0;
        (Vec2::new(view.x * 0.3, view.y * 0.2) / self.zoom).to_pos2() - self.pan + Vec2::splat(step)
    }

    fn palette(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new(t("Roles")).strong());
        ui.label(
            RichText::new(t("Claude Code Game Studios' agents. Click one to add it."))
                .small()
                .color(panels::MUTED),
        );
        ui.add_space(4.0);
        let running = self.running();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if ui
                .add_enabled(!running, egui::Button::new(t("+ Custom agent")))
                .clicked()
            {
                let id = self.org.add("", self.drop_point(ui));
                self.selected = vec![id];
            }
            for department in Department::ALL {
                egui::CollapsingHeader::new(
                    RichText::new(department.label()).color(department.color()),
                )
                .id_salt(("department", department as usize))
                .default_open(department == Department::Leadership)
                .show(ui, |ui| {
                    for role in ROLES.iter().filter(|role| role.department == department) {
                        let button = egui::Button::new(title(role.name)).frame(false);
                        if ui
                            .add_enabled(!running, button)
                            .on_hover_text(role.summary)
                            .clicked()
                        {
                            let id = self.org.add(role.name, self.drop_point(ui));
                            self.selected = vec![id];
                            self.group = None;
                        }
                    }
                });
            }
        });
    }

    /// Which organization is shown, its name, and adding or deleting one.
    fn organizations(&mut self, ui: &mut Ui) -> Result<(), String> {
        let mut result = Ok(());
        ui.add_enabled_ui(!self.running(), |ui| {
            ui.horizontal(|ui| {
                let mut pick = None;
                egui::ComboBox::from_id_salt("org_pick")
                    .selected_text(&self.name)
                    .show_ui(ui, |ui| {
                        for name in &self.names {
                            if ui.selectable_label(*name == self.name, name).clicked() {
                                pick = Some(name.clone());
                            }
                        }
                    });
                if let Some(name) = pick {
                    result = self.open(&name);
                }
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.rename)
                        .desired_width(160.0)
                        .hint_text(t("Name")),
                );
                if field.lost_focus() {
                    result = result.clone().and(self.rename_to_field());
                }
                if ui.button(t("+ New organization")).clicked() {
                    result = result.clone().and(self.add_organization());
                }
                ui.menu_button("...", |ui| {
                    if ui.button(t("Delete organization")).clicked() {
                        result = result.clone().and(self.delete_organization());
                        ui.close();
                    }
                });
            });
        });
        result
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let running = self.running();
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!running, |ui| {
                ui.menu_button(t("Templates"), |ui| {
                    if ui
                        .button(t("Small studio"))
                        .on_hover_text(t("Direction, design, code and testing in a line."))
                        .clicked()
                    {
                        self.org.starter();
                        ui.close();
                    }
                    if ui
                        .button(t("Claude Code Game Studios"))
                        .on_hover_text(t(
                            "Every CCGS agent: directors hand the task to their department leads, leads to their specialists.",
                        ))
                        .clicked()
                    {
                        self.org.ccgs();
                        ui.close();
                    }
                })
                .response
                .on_hover_text(t("Adds a ready-made team beside the agents already here."));
            });
            let can_group = !self.selected.is_empty() && !running;
            if ui
                .add_enabled(can_group, egui::Button::new(t("Group selection")))
                .on_hover_text(t("Puts a box around the selected agents."))
                .clicked()
            {
                self.group_selection();
            }
            if ui
                .add_enabled(can_group, egui::Button::new(t("Delete")))
                .clicked()
            {
                for id in std::mem::take(&mut self.selected) {
                    self.org.remove(id);
                }
            }
            if ui.button(t("Center view")).clicked() {
                self.pan = Vec2::ZERO;
                self.zoom = 1.0;
            }
            ui.label(
                RichText::new(
                    t("Drag from an agent's right pin to another agent to hand it the result. Right-click a pin to unlink it. Shift-click selects more. The mouse wheel zooms."),
                )
                .small()
                .color(panels::MUTED),
            );
            if let Some(refused) = self.refused {
                ui.colored_label(panels::ERROR, refused);
            }
        });
    }

    fn group_selection(&mut self) {
        let rects: Vec<Rect> = self
            .selected
            .iter()
            .filter_map(|&id| self.org.node(id))
            .map(|node| node.rect(Vec2::ZERO))
            .collect();
        let Some(bounds) = rects.into_iter().reduce(|a, b| a.union(b)) else {
            return;
        };
        let id = self.org.id();
        self.org.groups.push(Group {
            id,
            min: (bounds.min - Vec2::new(20.0, 20.0 + GROUP_TITLE)).into(),
            max: (bounds.max + Vec2::splat(20.0)).into(),
            ..Default::default()
        });
        self.group = Some(id);
        self.selected.clear();
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let (outer, background) =
            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        // The wheel zooms around the pointer, keeping what is under it there.
        // Popups over the canvas are layers of their own and keep the wheel.
        if ui.rect_contains_pointer(outer)
            && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
        {
            let (scroll, pinch) =
                ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
            let zoom = (self.zoom * pinch * (scroll / 200.0).exp()).clamp(0.25, 2.0);
            self.pan += (pointer - outer.min) * (1.0 / zoom - 1.0 / self.zoom);
            self.zoom = zoom;
        }
        if background.dragged() {
            self.pan += background.drag_delta() / self.zoom;
        }
        // The graph draws on its own layer, scaled about the canvas corner;
        // egui maps the pointer into it for the widgets there.
        let to_global = TSTransform::new(outer.min.to_vec2() * (1.0 - self.zoom), self.zoom);
        let layer = LayerId::new(ui.layer_id().order, ui.id().with("org_graph"));
        ui.ctx().set_sublayer(ui.layer_id(), layer);
        ui.ctx().set_transform_layer(layer, to_global);
        let rect = to_global.inverse() * outer;
        // Nodes panned past the edge neither draw over the toolbar nor take
        // its clicks: widgets only sense inside their clip rect.
        let ui = &mut ui.new_child(UiBuilder::new().layer_id(layer).max_rect(rect));
        ui.set_clip_rect(rect);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        if background.clicked() {
            self.selected.clear();
            self.group = None;
        }
        let origin = rect.min.to_vec2() + self.pan;
        // The grid moves with the view.
        let grid = 32.0;
        let line = Stroke::new(1.0, ui.visuals().faint_bg_color);
        let offset = Vec2::new(origin.x.rem_euclid(grid), origin.y.rem_euclid(grid));
        let mut x = rect.left() + offset.x;
        while x < rect.right() {
            painter.vline(x, rect.y_range(), line);
            x += grid;
        }
        let mut y = rect.top() + offset.y;
        while y < rect.bottom() {
            painter.hline(rect.x_range(), y, line);
            y += grid;
        }
        if self.org.nodes.is_empty() {
            painter.text(
                rect.center() - Vec2::new(0.0, 20.0),
                Align2::CENTER_CENTER,
                t("Add agents from the roles on the left."),
                FontId::proportional(14.0),
                panels::MUTED,
            );
            let button = Rect::from_center_size(
                rect.center() + Vec2::new(0.0, 14.0),
                Vec2::new(160.0, 26.0),
            );
            if ui
                .put(button, egui::Button::new(t("Start with a studio")))
                .clicked()
            {
                self.org.starter();
            }
        }
        let running = self.running();
        self.groups(ui, &painter, origin, running);
        self.links(ui, &painter, origin);
        let ids: Vec<u32> = self.org.nodes.iter().map(|node| node.id).collect();
        for id in ids {
            self.node(ui, rect, origin, id, running);
        }
        // A link dragged out of a node the wheel zoomed off the canvas ends
        // here: `node` skips nodes it does not draw.
        if self.wiring.is_some() && !ui.input(|input| input.pointer.any_down()) {
            self.wiring = None;
        }
    }

    fn groups(&mut self, ui: &mut Ui, painter: &egui::Painter, origin: Vec2, running: bool) {
        let mut removed = None;
        for index in 0..self.org.groups.len() {
            let group = &self.org.groups[index];
            let id = group.id;
            let rect = group.rect(origin);
            let [r, g, b] = group.color;
            let color = Color32::from_rgb(r, g, b);
            let picked = self.group == Some(id);
            painter.rect_filled(rect, 6.0, color.gamma_multiply(0.18));
            let stroke = Stroke::new(if picked { 2.0 } else { 1.0 }, color);
            painter.rect_stroke(rect, 6.0, stroke, StrokeKind::Inside);
            let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), GROUP_TITLE));
            painter.rect_filled(bar, 6.0, color.gamma_multiply(0.6));
            painter.text(
                bar.left_center() + Vec2::new(8.0, 0.0),
                Align2::LEFT_CENTER,
                &group.name,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            let title = ui.interact(
                bar,
                ui.id().with(("org_group", id)),
                Sense::click_and_drag(),
            );
            if title.clicked() || title.drag_started() {
                self.group = Some(id);
                self.selected.clear();
            }
            if title.dragged() && !running {
                let delta = title.drag_delta();
                // The nodes inside, by where their middle is.
                for node in &mut self.org.nodes {
                    if rect.contains(node.rect(origin).center()) {
                        node.pos = (Pos2::from(node.pos) + delta).into();
                    }
                }
                let group = &mut self.org.groups[index];
                group.min = (Pos2::from(group.min) + delta).into();
                group.max = (Pos2::from(group.max) + delta).into();
            }
            title.context_menu(|ui| {
                if ui.button(t("Remove group")).clicked() {
                    removed = Some(id);
                    ui.close();
                }
            });
            let corner = Rect::from_min_size(rect.max - Vec2::splat(14.0), Vec2::splat(14.0));
            let handle = ui
                .interact(corner, ui.id().with(("org_group_size", id)), Sense::drag())
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            painter.text(
                corner.center(),
                Align2::CENTER_CENTER,
                "◢",
                FontId::proportional(11.0),
                color,
            );
            if handle.dragged() {
                let group = &mut self.org.groups[index];
                let max = Pos2::from(group.max) + handle.drag_delta();
                let least = Pos2::from(group.min) + Vec2::new(120.0, GROUP_TITLE + 40.0);
                group.max = max.max(least).into();
            }
        }
        if let Some(id) = removed {
            self.org.groups.retain(|group| group.id != id);
            if self.group == Some(id) {
                self.group = None;
            }
        }
    }

    fn links(&mut self, ui: &mut Ui, painter: &egui::Painter, origin: Vec2) {
        let output = |node: &Node| node.rect(origin).min + Vec2::new(NODE_SIZE.x, HEADER / 2.0);
        let input = |node: &Node| node.rect(origin).min + Vec2::new(0.0, HEADER / 2.0);
        let curve = |from: Pos2, to: Pos2, color: Color32| {
            let bend = Vec2::new(((to.x - from.x).abs() * 0.5).max(40.0), 0.0);
            CubicBezierShape::from_points_stroke(
                [from, from + bend, to - bend, to],
                false,
                Color32::TRANSPARENT,
                Stroke::new(2.5, color),
            )
        };
        for &(from, to) in &self.org.links {
            if let (Some(from), Some(to)) = (self.org.node(from), self.org.node(to)) {
                let color = match from.state {
                    NodeState::Running | NodeState::Done => panels::GOOD,
                    _ => panels::ACCENT,
                };
                painter.add(curve(output(from), input(to), color));
            }
        }
        if let Some(from) = self.wiring.and_then(|id| self.org.node(id))
            && let Some(pointer) = graph_pointer(ui)
        {
            painter.add(curve(output(from), pointer, panels::WARNING));
        }
    }

    fn node(&mut self, ui: &mut Ui, canvas: Rect, origin: Vec2, id: u32, running: bool) {
        let Some(node) = self.org.node(id) else {
            return;
        };
        let rect = node.rect(origin);
        if !rect.intersects(canvas) {
            return;
        }
        let picked = self.selected.contains(&id);
        let painter = ui.painter_at(canvas);
        painter.rect_filled(
            rect.translate(Vec2::splat(3.0)),
            8.0,
            Color32::from_black_alpha(80),
        );
        painter.rect_filled(rect, 8.0, ui.visuals().window_fill);
        let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), HEADER));
        painter.rect_filled(
            header,
            egui::CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            node.color(),
        );
        let outline = match (picked, node.state) {
            (true, _) => Stroke::new(2.0, Color32::WHITE),
            (_, NodeState::Running) => Stroke::new(2.0, panels::GOOD),
            (_, NodeState::Failed) => Stroke::new(2.0, panels::ERROR),
            _ => Stroke::new(1.0, Color32::from_gray(0x30)),
        };
        painter.rect_stroke(rect, 8.0, outline, StrokeKind::Outside);
        painter.text(
            header.left_center() + Vec2::new(16.0, 0.0),
            Align2::LEFT_CENTER,
            &node.name,
            FontId::proportional(13.0),
            Color32::WHITE,
        );

        // Pins: what comes in on the left, the result going out on the right.
        let input_pin = header.left_center();
        let output_pin = header.right_center();
        for pin in [input_pin, output_pin] {
            painter.circle(
                pin,
                PIN / 1.4,
                ui.visuals().window_fill,
                Stroke::new(2.0, panels::ACCENT),
            );
        }
        let pin_rect = |at: Pos2| Rect::from_center_size(at, Vec2::splat(PIN * 2.4));
        let base = ui.id().with(("org_node", id));
        let inlet = ui
            .interact(pin_rect(input_pin), base.with("in"), Sense::click())
            .on_hover_text(t(
                "Takes the results of the agents linked into it. Right-click to unlink.",
            ));
        let outlet = ui
            .interact(
                pin_rect(output_pin),
                base.with("out"),
                Sense::click_and_drag(),
            )
            .on_hover_text(t(
                "Drag to an agent to hand it this one's result. Right-click to unlink.",
            ));
        if !running {
            if inlet.secondary_clicked() {
                self.org.links.retain(|&(_, to)| to != id);
            }
            if outlet.secondary_clicked() {
                self.org.links.retain(|&(from, _)| from != id);
            }
            if outlet.drag_started() {
                self.wiring = Some(id);
            }
        }
        if outlet.drag_stopped() && self.wiring.take() == Some(id) {
            let target = graph_pointer(ui).and_then(|pointer| {
                self.org
                    .nodes
                    .iter()
                    .rev()
                    .find(|node| node.rect(origin).expand(PIN).contains(pointer))
                    .map(|node| node.id)
            });
            if let Some(target) = target
                && target != id
                && !self.org.links.contains(&(id, target))
            {
                self.refused =
                    (!self.org.link(id, target)).then(|| t("That link would make a loop."));
            }
        }

        let title = ui.interact(
            header.shrink2(Vec2::new(PIN * 1.5, 0.0)),
            base.with("title"),
            Sense::click_and_drag(),
        );
        let shift = ui.input(|input| input.modifiers.shift);
        if title.clicked() || (title.drag_started() && !picked) {
            if shift {
                if picked {
                    self.selected.retain(|&other| other != id);
                } else {
                    self.selected.push(id);
                }
            } else {
                self.selected = vec![id];
            }
            self.group = None;
        }
        if title.dragged() && !running {
            let delta = title.drag_delta();
            let moving = if self.selected.contains(&id) {
                self.selected.clone()
            } else {
                vec![id]
            };
            for other in moving {
                if let Some(node) = self.org.node_mut(other) {
                    node.pos = (Pos2::from(node.pos) + delta).into();
                }
            }
        }
        let mut remove = false;
        title.context_menu(|ui| {
            if ui
                .add_enabled(!running, egui::Button::new(t("Delete agent")))
                .clicked()
            {
                remove = true;
                ui.close();
            }
        });
        if remove {
            self.org.remove(id);
            self.selected.retain(|&other| other != id);
            return;
        }

        let body = Rect::from_min_max(
            header.left_bottom() + Vec2::new(8.0, 6.0),
            rect.max - Vec2::splat(8.0),
        );
        let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt(("org_body", id)));
        child.shrink_clip_rect(rect);
        child.add_enabled_ui(!running, |ui| {
            let node = self.org.node_mut(id).expect("checked above");
            ui.spacing_mut().interact_size.y = 20.0;
            node_model_picker(ui, id, node);
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Effort")).small().color(panels::MUTED));
                egui::ComboBox::from_id_salt(("org_effort", id))
                    .selected_text(node.effort.label())
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for effort in Effort::ALL {
                            ui.selectable_value(&mut node.effort, effort, effort.label());
                        }
                    });
            });
        });
        let node = self.org.node(id).expect("checked above");
        child.horizontal(|ui| match node.state {
            NodeState::Idle => {
                let role = if node.role.is_empty() {
                    "custom"
                } else {
                    node.role.as_str()
                };
                ui.label(RichText::new(role).small().color(panels::MUTED));
            }
            NodeState::Waiting => {
                ui.label(RichText::new(t("waiting")).small().color(panels::MUTED));
            }
            NodeState::Running => {
                ui.spinner();
                ui.label(RichText::new(t("working")).small().color(panels::GOOD));
            }
            NodeState::Done => {
                ui.label(RichText::new(t("done")).small().color(panels::GOOD));
            }
            NodeState::Failed => {
                ui.label(RichText::new(t("failed")).small().color(panels::ERROR));
            }
        });
    }

    /// The picked agent's role and instructions, or the picked group.
    fn details(&mut self, ui: &mut Ui) {
        let running = self.running();
        if let Some(id) = self.picked_node()
            && let Some(node) = self.org.node_mut(id)
        {
            ui.add_space(4.0);
            ui.add_enabled_ui(!running, |ui| {
                ui.label(RichText::new(t("Agent")).strong());
                ui.add(egui::TextEdit::singleline(&mut node.name).desired_width(f32::INFINITY));
                if let Some(role) = find_role(&node.role) {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(role.department.label())
                                .small()
                                .color(role.department.color()),
                        );
                        ui.label(
                            RichText::new(format!(".claude/agents/{}.md", role.name))
                                .small()
                                .color(panels::MUTED),
                        );
                    });
                }
                ui.add_space(4.0);
                ui.label(RichText::new(t("Role")).small().color(panels::MUTED));
                ui.add(
                    egui::TextEdit::multiline(&mut node.instructions)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .hint_text(t("What this agent does in the team.")),
                );
                if node.provider == Provider::Gemini && node.effort != Effort::Default {
                    ui.colored_label(
                        panels::WARNING,
                        t("Gemini's CLI has no effort setting; it is left out."),
                    );
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(t("Last result")).small().color(panels::MUTED));
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if node.output.is_empty() {
                        ui.colored_label(panels::MUTED, t("Nothing yet."));
                    } else {
                        agent::reply(ui, &mut self.markdown, &node.output);
                    }
                });
        } else if let Some(id) = self.group
            && let Some(group) = self.org.groups.iter_mut().find(|group| group.id == id)
        {
            ui.add_space(4.0);
            ui.label(RichText::new(t("Group")).strong());
            ui.add(egui::TextEdit::singleline(&mut group.name).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label(t("Color"));
                ui.color_edit_button_srgb(&mut group.color);
            });
            if ui
                .button(t("Remove group"))
                .on_hover_text(t("Keeps the agents inside."))
                .clicked()
            {
                self.org.groups.retain(|group| group.id != id);
                self.group = None;
            }
        }
    }
}

fn read_ron<T: serde::de::DeserializeOwned>(file: &Path) -> Option<T> {
    ron::from_str(&fs::read_to_string(file).ok()?).ok()
}

fn write_ron(file: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let config = ron::ser::PrettyConfig::default().compact_arrays(true);
    let text = ron::ser::to_string_pretty(value, config).map_err(|error| error.to_string())?;
    fs::write(file, text).map_err(|error| error.to_string())
}

/// Puts `node`'s answer and thoughts so far in its message, the last of
/// `chat`.
fn show(node: &Node, chat: &mut [Message]) {
    let Some(message) = chat.last_mut() else {
        return;
    };
    message.text.clone_from(&node.output);
    message.thoughts.clone_from(&node.thoughts);
    if node.state == NodeState::Done && message.text.trim().is_empty() {
        message.text = "Done.".into();
    }
}

/// Every provider's models in one list, like the chat's picker, for one
/// node.
fn node_model_picker(ui: &mut Ui, id: u32, node: &mut Node) {
    let selected = format!(
        "{} · {}",
        node.provider.label(),
        agent::model_label(&node.model)
    );
    egui::ComboBox::from_id_salt(("org_model", id))
        .selected_text(selected)
        .width(ui.available_width())
        .height(f32::INFINITY)
        .show_ui(ui, |ui| {
            for provider in Provider::ALL {
                ui.label(RichText::new(provider.label()).strong());
                ui.indent(("org_model", id, provider.label()), |ui| {
                    for model in std::iter::once("").chain(provider.models().iter().copied()) {
                        let current = node.provider == provider && node.model == model;
                        if ui
                            .selectable_label(current, agent::model_label(model))
                            .clicked()
                        {
                            node.provider = provider;
                            node.model = model.into();
                        }
                    }
                });
            }
        });
}

/// Closing the project stops the run rather than let it edit on unseen.
impl Drop for OrgPanel {
    fn drop(&mut self) {
        self.stop();
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    fs::metadata(file)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> (Organization, [u32; 3]) {
        let mut org = Organization::default();
        let design = org.add("game-designer", Pos2::ZERO);
        let code = org.add("gameplay-programmer", Pos2::ZERO);
        let test = org.add("qa-tester", Pos2::ZERO);
        (org, [design, code, test])
    }

    #[test]
    fn runs_each_agent_after_the_ones_it_takes_results_from() {
        let (mut org, [design, code, test]) = studio();
        assert!(org.link(code, test));
        assert!(org.link(design, code));
        assert_eq!(org.order(), Ok(vec![design, code, test]));
        // A loop is refused and leaves the links as they were.
        assert!(!org.link(test, design));
        assert!(!org.link(design, design));
        assert_eq!(org.links.len(), 2);
        org.remove(code);
        assert!(org.links.is_empty());
        assert_eq!(org.order(), Ok(vec![design, test]));
    }

    #[test]
    fn hands_each_agent_its_role_the_task_and_what_came_before() {
        let (mut org, [design, code, _]) = studio();
        org.link(design, code);
        org.node_mut(design).unwrap().output = "Jumping costs stamina.".into();
        let input = org.input(code, "Add a jump.", 1);
        assert!(input.starts_with("You are Gameplay Programmer"));
        assert!(input.contains("Implements designed mechanics"));
        assert!(input.contains("Add a jump."));
        assert!(input.contains("## Game Designer\nJumping costs stamina."));
        assert!(input.contains("rather than asking whether you may"));
        assert!(
            !org.input(design, "Add a jump.", 1)
                .contains("before you reported")
        );
    }

    #[test]
    fn starts_a_role_with_its_tiers_model_and_effort() {
        let mut org = Organization::default();
        let director = org.add("creative-director", Pos2::ZERO);
        let lead = org.add("localization-lead", Pos2::ZERO);
        let specialist = org.add("prototyper", Pos2::ZERO);
        let custom = org.add("", Pos2::ZERO);
        let node = |id| org.node(id).unwrap();
        let tier = |id| (node(id).model.as_str(), node(id).effort);
        assert_eq!(tier(director), ("opus", Effort::High));
        assert_eq!(tier(lead), ("opus", Effort::Medium));
        assert_eq!(tier(specialist), ("sonnet", Effort::Default));
        assert_eq!(
            (node(custom).name.as_str(), node(custom).model.as_str()),
            ("Agent", "")
        );
        assert_eq!(Effort::High.args(Provider::Claude), ["--effort", "high"]);
        assert_eq!(
            Effort::Max.args(Provider::OpenAi),
            ["-c", "model_reasoning_effort=\"xhigh\""]
        );
        assert!(Effort::High.args(Provider::Gemini).is_empty());
        assert!(Effort::Default.args(Provider::Claude).is_empty());
    }

    #[test]
    fn moves_agents_still_on_ccgs_models_to_their_tier_once() {
        let mut org = Organization::default();
        let lead = org.add("lead-programmer", Pos2::ZERO);
        let writer = org.add("writer", Pos2::ZERO);
        let chosen = org.add("qa-lead", Pos2::ZERO);
        let gemini = org.add("qa-tester", Pos2::ZERO);
        for (id, model) in [
            (lead, "sonnet"),
            (writer, ""),
            (chosen, "haiku"),
            (gemini, ""),
        ] {
            let node = org.node_mut(id).unwrap();
            node.model = model.into();
            node.effort = Effort::Default;
        }
        org.node_mut(gemini).unwrap().provider = Provider::Gemini;
        let file = std::env::temp_dir().join(format!("besfa-org-v0-{}.besfa", std::process::id()));
        write_ron(&file, &org).unwrap();
        let mut back = Organization::read(&file);
        let model = |org: &Organization, id| org.node(id).unwrap().model.clone();
        assert_eq!(model(&back, lead), "opus");
        assert_eq!(model(&back, writer), "sonnet");
        assert_eq!(
            model(&back, chosen),
            "haiku",
            "a model the user picked stays"
        );
        assert_eq!(model(&back, gemini), "");
        // Once written at version 1, a CLI default picked after stays.
        back.node_mut(writer).unwrap().model.clear();
        back.write(&file).unwrap();
        let again = Organization::read(&file);
        fs::remove_file(&file).unwrap();
        assert_eq!(model(&again, writer), "");
    }

    #[test]
    fn keeps_the_organization_without_its_last_results() {
        let mut org = Organization::default();
        org.starter();
        org.nodes[0].output = "said something".into();
        let file = std::env::temp_dir().join(format!("besfa-org-{}.besfa", std::process::id()));
        org.write(&file).unwrap();
        let back = Organization::read(&file);
        fs::remove_file(&file).unwrap();
        assert_eq!(back.nodes.len(), 4);
        assert_eq!(back.links.len(), 3);
        assert_eq!(back.groups[0].name, "Studio");
        assert!(back.nodes[0].output.is_empty());
        assert_eq!(back.next_id, org.next_id);
    }

    #[test]
    fn the_ccgs_template_is_every_role_in_three_tiers_beside_what_is_there() {
        let mut org = Organization::default();
        org.starter();
        org.ccgs();
        let ccgs = &org.nodes[4..];
        assert_eq!(ccgs.len(), ROLES.len(), "every role once");
        assert!(
            ROLES
                .iter()
                .all(|role| ccgs.iter().any(|node| node.role == role.name))
        );
        // Each agent takes the task from exactly one above it, directors
        // from none.
        for node in ccgs {
            let above = org.links.iter().filter(|&&(_, to)| to == node.id).count();
            let director = CCGS_CHART.iter().any(|&(boss, _, _)| boss == node.role);
            assert_eq!(above, usize::from(!director), "{}", node.role);
        }
        let order = org.order().unwrap();
        let place = |role: &str| {
            let id = ccgs.iter().find(|node| node.role == role).unwrap().id;
            order.iter().position(|&other| other == id).unwrap()
        };
        assert!(place("technical-director") < place("lead-programmer"));
        assert!(place("lead-programmer") < place("bevy-specialist"));
        // Right of the starter studio, not over it.
        let studio_right = org.groups[0].max[0];
        assert!(ccgs.iter().all(|node| node.pos[0] > studio_right));
        assert_eq!(org.groups.len(), 4);
    }

    #[test]
    fn hands_each_result_on_and_stops_at_a_failure() {
        let (mut org, [design, code, test]) = studio();
        org.link(design, code);
        org.link(code, test);
        org.node_mut(design).unwrap().provider = Provider::OpenAi;
        let besfa = std::env::temp_dir().join(format!("besfa-org-run-{}", std::process::id()));
        let _ = fs::remove_dir_all(&besfa);
        let dir = besfa.join("organizations");
        let mut panel = OrgPanel::new(dir.clone());
        panel.org = org;
        // On disk, as an organization a run goes through is.
        panel.unsaved = true;
        panel.save().unwrap();
        // The designer logs noise and leaves its answer in a file, as codex
        // does; the programmer prints what it was sent; the tester fails.
        let answer =
            std::env::temp_dir().join(format!("besfa-org-answer-{}.txt", std::process::id()));
        let design_run = (
            "cmd".into(),
            vec![
                "/c".into(),
                format!(
                    // No quotes: cmd would take them as part of the path.
                    "echo Jumps cost stamina.>{}& echo noisy log",
                    answer.display()
                ),
            ],
            vec![],
        );
        let echo = ("findstr".into(), vec!["/r".into(), ".".into()], vec![]);
        let fail = ("cmd".into(), vec!["/c".into(), "exit 3".into()], vec![]);
        panel.run = Some(Run {
            organization: "Organization".into(),
            task: "Add a jump.".into(),
            files: Vec::new(),
            chat: 7,
            order: vec![design, code, test],
            invocations: vec![design_run, echo.clone(), fail],
            next: 0,
            round: 1,
            process: None,
            answers: vec![Some(answer.clone()), None, None],
            stopped: false,
            directory: ".".into(),
            watched: ("none".into(), None),
        });
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut finished = None;
        let mut chat = Vec::new();
        while finished.is_none() && std::time::Instant::now() < deadline {
            finished = panel.poll(&ctx, (&mut chat, &mut Vec::new()));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(finished, Some(false));
        assert!(!panel.running());
        let node = |id| panel.org.node(id).unwrap();
        assert_eq!(node(design).state, NodeState::Done);
        assert_eq!(node(design).output.trim(), "Jumps cost stamina.");
        assert!(!answer.exists());
        // The programmer was sent the designer's answer, without its log.
        assert!(
            node(code)
                .output
                .contains("## Game Designer\nJumps cost stamina.")
        );
        assert!(!node(code).output.contains("noisy log"));
        assert_eq!(node(test).state, NodeState::Failed);
        assert!(node(test).output.contains("code 3"));
        // Each answered in the chat under its own name, the designer's log
        // folded away as thoughts.
        let said: Vec<&str> = chat
            .iter()
            .map(|message| message.speaker.as_ref().unwrap().0.as_str())
            .collect();
        assert_eq!(said, ["Game Designer", "Gameplay Programmer", "Qa Tester"]);
        assert_eq!(chat[0].text.trim(), "Jumps cost stamina.");
        assert!(chat[0].thoughts.contains("noisy log"));
        assert!(chat[2].text.contains("code 3"));

        // The run stays paused, in the project too, where a restarted
        // editor finds it with its chat.
        let paused = panel.paused().unwrap().clone();
        let done: Vec<u32> = paused.done.iter().map(|done| done.0).collect();
        assert_eq!(done, [design, code]);
        assert_eq!(paused.chat(), (7, chat.as_slice()));
        assert_eq!(OrgPanel::new(dir.clone()).paused(), Some(&paused));

        // Continue runs only the tester, handing it the programmer's answer.
        // The designer's Codex is logged out by now; it is done, so that
        // does not matter.
        let agents = Agents::fixed(
            [
                agent::Cli::LoggedIn,
                agent::Cli::LoggedOut,
                agent::Cli::LoggedIn,
            ],
            &[],
        );
        panel
            .resume(
                &AgentSettings::default(),
                &agents,
                Path::new("."),
                "none".into(),
            )
            .unwrap();
        panel.run.as_mut().unwrap().invocations = vec![echo.clone(), echo.clone(), echo];
        let mut finished = None;
        while finished.is_none() && std::time::Instant::now() < deadline {
            finished = panel.poll(&ctx, (&mut chat, &mut Vec::new()));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(finished, Some(false));
        assert_eq!(chat.len(), 4);
        assert_eq!(chat[3].speaker.as_ref().unwrap().0, "Qa Tester");
        assert!(chat[3].text.contains("## Gameplay Programmer\n"));
        assert_eq!(
            panel.org.node(design).unwrap().output.trim(),
            "Jumps cost stamina."
        );
        // Done: nothing left to continue.
        assert!(panel.paused().is_none());
        assert!(!besfa.join(RUN_FILE).exists());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn runs_again_who_says_more_and_who_comes_after_up_to_the_last_round() {
        for more in [
            "Planned.\r\n\r\n**STATUS: MORE**\r\n",
            "**Status:** MORE",
            "- STATUS:  **more** (waiting on QA).",
            "`STATUS:MORE`\n---",
            "STATUS: MORE\n\n## For the user\nPick a font.",
        ] {
            assert!(says_more(more), "{more:?}");
        }
        for done in [
            "Planned.\nSTATUS: DONE",
            "STATUS: MORE\nfixed it after all\nSTATUS: DONE",
            "End with STATUS: DONE or STATUS: MORE.",
            "The designer said:\n> STATUS: MORE",
            "Status: Moreover, all done.",
            "",
        ] {
            assert!(!says_more(done), "{done:?}");
        }

        let (mut org, [design, code, test]) = studio();
        org.link(design, code);
        let besfa = std::env::temp_dir().join(format!("besfa-org-rounds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&besfa);
        let dir = besfa.join("organizations");
        let mut panel = OrgPanel::new(dir.clone());
        panel.org = org;
        panel.unsaved = true;
        panel.save().unwrap();
        // The designer always has more; the programmer prints what it was
        // sent less its status lines, so it never says MORE itself and runs
        // again only for coming after the designer; the tester, linked to
        // no one, is done.
        let more = (
            "cmd".into(),
            vec!["/c".into(), "echo Planned.& echo STATUS: MORE".into()],
            vec![],
        );
        let echo = (
            "findstr".into(),
            vec!["/v".into(), "/c:STATUS:".into()],
            vec![],
        );
        let done = (
            "cmd".into(),
            vec!["/c".into(), "echo Tested.".into()],
            vec![],
        );
        let invocations = vec![more, echo, done];
        panel.run = Some(Run {
            organization: "Organization".into(),
            task: "Add a jump.".into(),
            files: Vec::new(),
            chat: 3,
            order: vec![design, code, test],
            invocations: invocations.clone(),
            next: 0,
            round: 1,
            process: None,
            answers: vec![None, None, None],
            stopped: false,
            directory: ".".into(),
            watched: ("none".into(), None),
        });
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut chat: Vec<Message> = Vec::new();
        let poll = |panel: &mut OrgPanel,
                    chat: &mut Vec<Message>,
                    until: &dyn Fn(&OrgPanel, &[Message]) -> bool| {
            let mut finished = None;
            while finished.is_none() && !until(panel, chat) && std::time::Instant::now() < deadline
            {
                finished = panel.poll(&ctx, (chat, &mut Vec::new()));
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            finished
        };
        // Stopped while the designer works in round 2, it pauses in round 2.
        poll(&mut panel, &mut chat, &|panel, _| {
            panel
                .run
                .as_ref()
                .is_some_and(|run| run.round == 2 && run.process.is_some())
        });
        panel.stop();
        assert_eq!(poll(&mut panel, &mut chat, &|_, _| false), Some(false));
        let paused = panel.paused().unwrap().clone();
        assert_eq!(paused.round, 2);
        let last: Vec<u32> = paused.last.iter().map(|last| last.0).collect();
        assert_eq!(last, [design, code], "the tester is not running again");
        let agents = Agents::fixed([agent::Cli::LoggedIn; 3], &[]);
        panel
            .resume(
                &AgentSettings::default(),
                &agents,
                Path::new("."),
                "none".into(),
            )
            .unwrap();
        panel.run.as_mut().unwrap().invocations = invocations;
        assert_eq!(panel.run.as_ref().unwrap().round, 2);
        assert_eq!(poll(&mut panel, &mut chat, &|_, _| false), Some(false));

        let mut said: Vec<&str> = chat
            .iter()
            .map(|message| message.speaker.as_ref().unwrap().0.as_str())
            .collect();
        // The designer says it twice if Stop came before it was done.
        said.dedup();
        assert_eq!(
            said,
            [
                "Game Designer",
                "Gameplay Programmer",
                "Qa Tester",
                "Organization",
                "Game Designer",
                "Gameplay Programmer",
                "Organization",
                "Game Designer",
                "Gameplay Programmer",
            ],
            "round 3 is the last"
        );
        assert!(chat[3].text.contains("2 of 3"));
        let code = &panel.org.node(code).unwrap().output;
        assert!(code.contains("This is round 3 of at most 3."));
        assert!(code.contains("No round follows this one"));
        assert!(code.contains("## Game Designer\nPlanned."));
        assert!(!says_more(code), "it ran again only for coming after");
        assert!(panel.paused().is_none());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn reads_what_claude_says_on_the_way_and_its_answer() {
        assert_eq!(
            streamed(
                r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":""},{"type":"tool_use","name":"Bash","input":{"command":"cargo test\nmore","description":"Run tests"}}]}}"#
            ),
            Streamed::Thought("[Bash] cargo test".into())
        );
        assert_eq!(
            streamed(r#"{"type":"system","subtype":"init"}"#),
            Streamed::Nothing
        );
        assert_eq!(
            streamed("Error: not logged in"),
            Streamed::Text("Error: not logged in".into())
        );
        let mut node = Node::default();
        node.take(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Checking the player."}]}}"#);
        node.take(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Jump added."}]}}"#,
        );
        node.take(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Jump added."}"#,
        );
        assert_eq!(node.output, "Jump added.");
        assert_eq!(node.thoughts, "Checking the player.");
    }

    #[test]
    fn a_stop_between_two_agents_starts_no_more() {
        let (org, [design, code, _]) = studio();
        let besfa = std::env::temp_dir().join(format!("besfa-org-stop-{}", std::process::id()));
        let mut panel = OrgPanel::new(besfa.join("organizations"));
        panel.org = org;
        let echo = || ("findstr".into(), vec!["/r".into(), ".".into()], vec![]);
        panel.run = Some(Run {
            organization: "Organization".into(),
            task: "Add a jump.".into(),
            files: Vec::new(),
            chat: 1,
            order: vec![design, code],
            invocations: vec![echo(), echo()],
            next: 1,
            round: 1,
            process: None,
            answers: vec![None, None],
            stopped: false,
            directory: ".".into(),
            watched: ("none".into(), None),
        });
        panel.stop();
        assert_eq!(
            panel.poll(
                &egui::Context::default(),
                (&mut Vec::new(), &mut Vec::new())
            ),
            Some(false)
        );
        assert_eq!(panel.org.node(code).unwrap().state, NodeState::Idle);
        assert!(panel.paused().is_some());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn drops_links_the_editor_cannot_make_when_reading() {
        let (mut org, [design, code, _]) = studio();
        org.links = vec![(design, code), (design, code), (code, design), (99, code)];
        let twin = Node {
            id: code,
            ..Default::default()
        };
        org.nodes.push(twin);
        org.next_id = 1;
        let mut org = org.tidy();
        assert_eq!(org.links, [(design, code)]);
        assert_eq!(org.nodes.len(), 3);
        assert!(org.order().is_ok());
        let id = org.id();
        assert!(org.node(id).is_none(), "a new id is not one in use");
    }

    #[test]
    fn starts_a_studio_and_groups_it() {
        use egui_kittest::{Harness, kittest::Queryable};
        let dir = std::env::temp_dir().join(format!("besfa-org-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("Organization.besfa");
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 700.0))
            .build_ui_state(
                |ui, panel: &mut OrgPanel| drop(panel.ui(ui)),
                OrgPanel::new(dir.clone()),
            );
        harness.run();
        harness.get_by_label("Start with a studio").click();
        harness.run();
        assert_eq!(harness.state().org.nodes.len(), 4);
        // Written once the click is over.
        assert_eq!(Organization::read(&file).nodes.len(), 4);

        harness.get_by_label("Producer").click();
        harness.run();
        assert_eq!(harness.state().org.nodes.len(), 5);
        assert_eq!(harness.state().selected.len(), 1);
        harness.get_by_label("Group selection").click();
        harness.run();
        assert_eq!(harness.state().org.groups.len(), 2);

        harness.get_by_label("+ New organization").click();
        harness.run();
        let panel = harness.state();
        assert_eq!(panel.names, ["Organization", "Organization 2"]);
        assert!(panel.org.nodes.is_empty());
        assert!(dir.join("Organization 2.besfa").is_file());
        assert_eq!(Organization::read(&file).nodes.len(), 5);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_paused_run_follows_its_organization_renamed_and_goes_with_it_deleted() {
        let besfa = std::env::temp_dir().join(format!("besfa-paused-org-{}", std::process::id()));
        let _ = fs::remove_dir_all(&besfa);
        let dir = besfa.join("organizations");
        let mut panel = OrgPanel::new(dir.clone());
        panel.unsaved = true;
        panel.save().unwrap();
        panel.paused = Some(Paused {
            organization: "Organization".into(),
            chat: 3,
            ..Default::default()
        });
        panel.rename = "Team".into();
        panel.rename_to_field().unwrap();
        assert_eq!(panel.paused().unwrap().organization(), "Team");
        assert_eq!(
            OrgPanel::new(dir.clone()).paused().unwrap().organization(),
            "Team"
        );
        // One whose organization went from the folder behind the editor's
        // back is dropped.
        fs::remove_file(dir.join("Team.besfa")).ok();
        assert!(OrgPanel::new(dir.clone()).paused().is_none());
        // Deleted, a new organization of the same name and ids must not
        // take its answers.
        panel.delete_organization().unwrap();
        assert!(panel.paused().is_none());
        assert!(!besfa.join(RUN_FILE).exists());
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn keeps_each_organization_in_a_file_named_after_it() {
        let besfa = std::env::temp_dir().join(format!("besfa-orgs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&besfa);
        let dir = besfa.join("organizations");
        // A project's one organization from before moves in.
        let mut old = Organization::default();
        old.starter();
        old.write(&besfa.join("organization.besfa")).unwrap();
        let mut panel = OrgPanel::new(dir.clone());
        assert!(!besfa.join("organization.besfa").exists());
        assert_eq!(panel.names, ["Organization"]);
        assert_eq!(panel.org.nodes.len(), 4);

        panel.add_organization().unwrap();
        panel.org.add("producer", Pos2::ZERO);
        panel.unsaved = true;
        panel.rename = "QA team".into();
        panel.rename_to_field().unwrap();
        assert!(panel.save().is_ok());
        assert_eq!(panel.names, ["Organization", "QA team"]);
        assert_eq!(
            Organization::read(&dir.join("QA team.besfa")).nodes.len(),
            1
        );
        assert!(!dir.join("Organization 2.besfa").exists());

        // A taken or unusable name is refused and the field goes back.
        for name in ["Organization", "organization", "a/b", " "] {
            panel.rename = name.into();
            panel.rename_to_field().unwrap();
            assert_eq!(panel.name, "QA team");
            assert_eq!(panel.rename, "QA team");
            assert!(panel.refused.is_some());
        }

        panel.rename = "QA Team".into();
        panel.rename_to_field().unwrap();
        assert_eq!(panel.names, ["Organization", "QA Team"]);
        panel.rename = "QA team".into();
        panel.rename_to_field().unwrap();

        let reopened = OrgPanel::new(dir.clone());
        assert_eq!(reopened.names, ["Organization", "QA team"]);
        panel.open("Organization").unwrap();
        assert_eq!(panel.org.nodes.len(), 4);
        panel.delete_organization().unwrap();
        assert_eq!((panel.name.as_str(), panel.org.nodes.len()), ("QA team", 1));
        panel.delete_organization().unwrap();
        // The last one gone leaves a new empty one.
        assert_eq!(panel.names, ["Organization"]);
        assert!(panel.org.nodes.is_empty());
        assert!(
            panel
                .start(
                    "QA team",
                    "Test it.".into(),
                    &[],
                    1,
                    &AgentSettings::default(),
                    &Agents::fixed([agent::Cli::LoggedIn; 3], &[]),
                    &besfa,
                    "none".into(),
                )
                .is_err()
        );
        fs::remove_dir_all(&besfa).unwrap();
    }

    #[test]
    fn the_wheel_zooms_around_the_pointer() {
        use egui_kittest::{Harness, kittest::Queryable};
        let file = std::env::temp_dir().join(format!(
            "besfa-org-zoom-{}/organizations",
            std::process::id()
        ));
        let mut panel = OrgPanel::new(file.clone());
        panel.org = Organization::default();
        panel.org.starter();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 700.0))
            .build_ui_state(|ui, panel: &mut OrgPanel| drop(panel.ui(ui)), panel);
        let pointer = egui::pos2(600.0, 350.0);
        let wheel = |harness: &mut Harness<OrgPanel>, notches: f32| {
            harness.event(egui::Event::PointerMoved(pointer));
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, notches),
                phase: egui::TouchPhase::Move,
                modifiers: Default::default(),
            });
            // Smooth scrolling hands the wheel out over a few frames.
            for _ in 0..30 {
                harness.step();
            }
        };
        harness.run();
        let before = harness.state().pan;
        wheel(&mut harness, 2.0);
        let zoomed = harness.state().zoom;
        assert!(zoomed > 1.2, "{zoomed}");
        // Out again around the same point lands where it started.
        wheel(&mut harness, -2.0);
        let state = harness.state();
        assert!((state.zoom - 1.0).abs() < 0.01, "{}", state.zoom);
        assert!((state.pan - before).length() < 1.0, "{:?}", state.pan);
        wheel(&mut harness, -40.0);
        assert_eq!(harness.state().zoom, 0.25);
        harness.get_by_label("Center view").click();
        harness.run();
        assert_eq!(harness.state().zoom, 1.0);

        // A link whose node was zoomed off the canvas still ends on release.
        let from = harness.state().org.nodes[0].id;
        harness.state_mut().wiring = Some(from);
        harness.state_mut().pan = Vec2::new(-5000.0, 0.0);
        harness.run();
        assert_eq!(harness.state().wiring, None);
        let _ = fs::remove_dir_all(&file);
    }
}
