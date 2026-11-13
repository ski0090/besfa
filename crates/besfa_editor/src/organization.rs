//! The Agent window's Organization tab: agents with roles, wired like a
//! Blueprint graph. A link hands one agent's result to the next, and a task
//! sent to the organization from the chat runs every agent once, in link
//! order, those whose earlier agents are done side by side in git
//! worktrees of their own, each answering in the chat. The roles to start from are Claude
//! Code Game Studios' (CCGS) agents.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::SystemTime,
};

use eframe::egui::{
    self, Align2, Color32, FontId, LayerId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, emath::TSTransform, epaint::CubicBezierShape,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod roles_tree;
mod target;

use crate::{
    agent::{self, AgentSettings, Agents, Ask, Message, Provider},
    i18n::t,
    panels,
    process::{self, CliProcess, Output},
};

/// Where a project keeps its organizations, one file each, named after it.
pub const ORGANIZATIONS_DIR: &str = "besfa/organizations";

/// Next to that folder: the last run, while it has agents left to go.
const RUN_FILE: &str = "organization-run.ron";

/// Next to it too: the roles tree, which every organization shares.
const TREE_FILE: &str = "roles.ron";

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

    /// The English name, which is also a folder's name in the roles tree.
    fn key(self) -> &'static str {
        match self {
            Department::Leadership => "Leadership",
            Department::Design => "Design",
            Department::Programming => "Programming",
            Department::ArtAudio => "Art & Audio",
            Department::Quality => "Quality",
            Department::Operations => "Operations",
        }
    }

    fn label(self) -> &'static str {
        t(self.key())
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

    /// Adds an agent at `pos` from one saved in the roles tree.
    fn add_saved(&mut self, saved: roles_tree::Saved, pos: Pos2) -> u32 {
        let id = self.id();
        self.nodes.push(Node {
            id,
            name: saved.name,
            instructions: saved.instructions,
            provider: saved.provider,
            model: saved.model,
            effort: saved.effort,
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
    /// With `merge` it also reviews and merges the pull requests it opens.
    /// In a `parallel` run it hears that agents work side by side, and
    /// in a `worktree` where it works and what becomes of it.
    fn input(
        &self,
        id: u32,
        task: &str,
        round: u32,
        merge: bool,
        parallel: bool,
        worktree: Option<&Worktree>,
    ) -> String {
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
        // Agents later in the order see only reports, so whoever opens a
        // pull request is the one who sees it through. ponytail: a reviewer
        // agent of its own if self-review proves too thin.
        if merge {
            input.push_str(
                "\n\nThe user also lets this run merge its own pull requests and close the issues they finish, which is their approval for both even where the project says to ask first. Before you finish, see each pull request you opened through: review its whole diff (`gh pr diff`) as a reviewer who did not write it would, through a fresh subagent if you can start one, fix and push what must change, and review again until nothing must. Then merge it with `gh pr merge --merge`, unless the project names another method, once its checks pass, or right away if it has none. Merge stacked pull requests oldest first; before merging one whose base branch has merged, point it at the branch that one merged into (`gh pr edit <number> --base <branch>`), and start a new branch from there rather than from a merged one. Never use `--admin` or `--auto`: a merge GitHub refuses stays open. GitHub does not let you approve your own pull request, so do not try. When the task comes from a GitHub issue, write `Closes #<issue>` (never in a commit message) in the description of the pull request that will merge last for it and `Part of #<issue>` in the others, so merging the last one closes the issue. Once everything the issue asks for has merged and nothing is left for the user, close it with `gh issue close <issue> --comment` naming the pull requests if nothing closed it. An issue with any part left stays open: reopen one a merge closed too early (`gh issue reopen <issue>`). Say in your report what you merged and closed, and under \"For the user\" why any pull request you opened stays open.",
            );
        }
        // Worktrees start from the project's last commit.
        if parallel {
            input.push_str(
                "\n\nSome agents of this run work side by side. Commit what you change before you finish: an agent after you may start in a git worktree made from the last commit, and does not see uncommitted changes.",
            );
        }
        if let Some(worktree) = worktree {
            input.push_str(&format!(
                "\n\nYou work beside other agents, in a git worktree of your own: {}, on branch {}, made from {}. Work and commit only there. Do not switch branches, push {}, or open a pull request from it. When you finish, the editor commits what you left, merges {} into {} in the project, and removes the worktree and the branch, so the agents after you get your work.",
                worktree.dir.display(),
                worktree.branch,
                worktree.base,
                worktree.branch,
                worktree.branch,
                worktree.base
            ));
            // Other features make other builds of the dependencies, which
            // the worktree's folder does not have.
            let manifest = fs::read_to_string(worktree.dir.join("Cargo.toml")).unwrap_or_default();
            input.push_str(&format!(
                " Cargo builds here in {}, which CARGO_TARGET_DIR names: it was filled from what the project already built, so only this repository's packages compile. Keep it, and build, run and test with `--features {}` as the editor does; other features build every dependency again from scratch.",
                worktree.target.display(),
                besfa_protocol::run_features(&manifest),
            ));
        }
        input
    }

    /// The agents of `order` waiting to run whose earlier agents are all
    /// done.
    fn ready(&self, order: &[u32]) -> Vec<u32> {
        let done = |id| {
            self.node(id)
                .is_some_and(|node| node.state == NodeState::Done)
        };
        order
            .iter()
            .copied()
            .filter(|&id| {
                self.node(id).is_some_and(|node| {
                    !matches!(
                        node.state,
                        NodeState::Done | NodeState::Running | NodeState::Failed
                    )
                }) && self
                    .links
                    .iter()
                    .filter(|&&(_, to)| to == id)
                    .all(|&(from, _)| done(from))
            })
            .collect()
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
        .find_map(|line| marked(line, "STATUS:"))
        .is_some_and(|word| {
            let word: String = word.chars().take_while(char::is_ascii_alphabetic).collect();
            word.eq_ignore_ascii_case("MORE")
        })
}

/// What follows `marker` on `line`, if the line starts with it. Markdown
/// and spaces around it are let go, and case too; a quoted line (`> ...`)
/// is someone else's.
fn marked(line: &str, marker: &str) -> Option<String> {
    let bare: String = line.chars().filter(|c| !"*`_".contains(*c)).collect();
    let bare = bare.trim_start_matches(|c: char| c.is_whitespace() || "#-".contains(c));
    let head = bare.get(..marker.len())?;
    head.eq_ignore_ascii_case(marker)
        .then(|| bare[marker.len()..].trim().to_string())
}

/// The organization a chat agent handed its task to, read like a status
/// line from the last line of its answer, `ORGANIZATION: <name>`, and the
/// answer without that line: the brief it wrote for the team. Only the
/// last line counts, a closing code fence or rule after it let go, so a
/// marker the agent quotes on the way hands nothing over.
pub fn handoff(output: &str) -> Option<(String, String)> {
    let mut lines: Vec<&str> = output.lines().collect();
    let at = lines.iter().rposition(|line| {
        line.chars()
            .any(|c| !c.is_whitespace() && !"*`_-#~".contains(c))
    })?;
    marked(lines[at], "ORGANIZATION:")?;
    // From the line as written, so an `_` inside the name stays, and after
    // its first colon, as Markdown may come before it (`**ORGANIZATION**:`).
    // Names hold no colon.
    let (_, name) = lines[at].split_once(':')?;
    let name = name.trim_matches(|c: char| c.is_whitespace() || "*`_\"'.<>".contains(c));
    if name.is_empty() {
        return None;
    }
    lines.remove(at);
    Some((name.into(), lines.join("\n").trim().into()))
}

/// A task going through the organization. The agents whose earlier agents
/// are done start together, each in a git worktree of its own; one that
/// would work alone works in the project folder, and none start beside it.
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
    /// Which time through the agents this is, from 1.
    round: u32,
    /// The agents at work.
    running: Vec<Running>,
    /// Where each agent's CLI writes its final answer, when it can, by its
    /// place in `order`. Codex logs the run, prompt included, on stderr.
    answers: Vec<Option<PathBuf>>,
    stopped: bool,
    /// An agent failed: the others at work finish, and no more start.
    halted: bool,
    directory: PathBuf,
    /// A file the run may change, and when it was changed before the run.
    watched: (PathBuf, Option<SystemTime>),
    /// Whether its agents review and merge the pull requests they open.
    merge: bool,
    /// Whether the project is in git. Without it the agents go one at a
    /// time, all in the project folder.
    git: bool,
    /// Whether some of its agents can work side by side.
    parallel: bool,
    /// When the run first started, in seconds, which names its worktrees
    /// so a resume finds them again.
    started: u64,
    /// What the project folder held beyond its commit when the run began.
    /// Once an agent leaves more there, worktrees made from the commit
    /// would miss it, so the agents go one at a time in the folder.
    untouched: u64,
    /// Whether an agent left changes there, so the rest of the run goes
    /// one at a time.
    one_at_a_time: bool,
}

/// What the project folder holds beyond its commit, as a number that
/// changes when that does. ponytail: of an untracked file only its name
/// counts, not what is in it.
fn uncommitted(project: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    // The editor writes where the run is there, as it goes.
    let run_file = format!(":(exclude)besfa/{RUN_FILE}");
    let paths = ["--", ".", run_file.as_str()];
    let mut hasher = std::hash::DefaultHasher::new();
    git(
        project,
        &[&["status", "--porcelain", "-uall"][..], &paths].concat(),
    )
    .hash(&mut hasher);
    git(project, &[&["diff", "HEAD"][..], &paths].concat()).hash(&mut hasher);
    hasher.finish()
}

/// An agent at work.
struct Running {
    id: u32,
    /// None until its worktree's target folder is filled, when `waiting`
    /// starts it.
    process: Option<CliProcess>,
    waiting: Option<Waiting>,
    /// Its message in the chat.
    message: usize,
    /// Its own checkout, when it works beside other agents.
    worktree: Option<Worktree>,
    /// The ids of what it asked the user, waiting for answers.
    asked: Vec<String>,
}

/// Where the editor's game stands, which decides whether agents may work in
/// worktrees, whose target folders are filled from the project's build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameBuild {
    /// Not running and not being built: its build failed, or none started.
    Missing,
    /// Being built or launched, as after a code change.
    Building,
    /// Running, built from the project folder.
    Built,
}

/// An agent that starts once its worktree's target folder is filled from
/// the project's, which takes seconds.
struct Waiting {
    /// Why the folder is empty, if it could not be filled.
    filled: mpsc::Receiver<Result<(), String>>,
    directory: PathBuf,
    invocation: Invocation,
    input: String,
}

impl Running {
    /// What its CLI wrote since the last call. One still waiting starts
    /// once its folder is filled, with why it could not be in `node`'s
    /// thoughts; one that cannot start, or that the run stopped first,
    /// exits.
    fn poll(&mut self, stopped: bool, ctx: &egui::Context, node: &mut Node) -> Vec<Output> {
        if let Some(process) = &self.process {
            return process.poll();
        }
        let Some(waiting) = &self.waiting else {
            return vec![Output::Exit(1)];
        };
        if stopped {
            // Filling goes on, and a Continue finds the folder filled.
            self.waiting = None;
            return vec![Output::Exit(1)];
        }
        let filled = match waiting.filled.try_recv() {
            Ok(filled) => filled,
            Err(mpsc::TryRecvError::Empty) => return Vec::new(),
            Err(mpsc::TryRecvError::Disconnected) => Err("it stopped early.".into()),
        };
        if let Err(why) = filled {
            push_line(
                &mut node.thoughts,
                &format!("{} {why}", t("Its builds start from scratch:")),
            );
        }
        let waiting = self.waiting.take().expect("checked above");
        let (program, args, env) = &waiting.invocation;
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        match CliProcess::start(&waiting.directory, program, &args, &env, ctx) {
            Ok(mut process) => {
                agent::send_prompt(&mut process, program, waiting.input);
                self.process = Some(process);
                Vec::new()
            }
            Err(error) => {
                push_line(
                    &mut node.output,
                    &format!("Could not start {program}: {error}"),
                );
                vec![Output::Exit(1)]
            }
        }
    }
}

/// An agent's own checkout while it works beside other agents: a git
/// worktree next to the project, on a branch of its own.
struct Worktree {
    path: PathBuf,
    /// Where the project sits in it, when the project is a folder inside
    /// its repository.
    dir: PathBuf,
    branch: String,
    /// The project's branch it was made from, and is merged back into.
    base: String,
    /// Where cargo builds there: its own folder, since every checkout
    /// gives the game the same build names (see `target`).
    target: PathBuf,
}

/// Runs git in `dir`: what it printed, or what it said when it failed.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = process::command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().into())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().into())
    }
}

/// Where agent `id`'s worktree for the run `started` in `project` goes,
/// and its branch: next to the repository's own folder, so never inside
/// it when the project is a folder of a bigger one.
fn worktree_place(project: &Path, started: u64, id: u32) -> Result<(PathBuf, String), String> {
    let top = PathBuf::from(git(project, &["rev-parse", "--show-toplevel"])?);
    let name = top
        .file_name()
        .map_or_else(|| "project".into(), |name| name.to_string_lossy());
    let path = top.with_file_name(format!("{name}-org-{started}-{id}"));
    Ok((path, format!("besfa-org/{started}-{id}")))
}

/// Whether a stopped run left agent `id` a worktree or a branch to go on
/// with.
fn worktree_kept(project: &Path, started: u64, id: u32) -> bool {
    worktree_place(project, started, id).is_ok_and(|(path, branch)| {
        path.exists()
            || git(
                project,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            )
            .is_ok()
    })
}

/// Agent `id`'s worktree for the run `started` in `project`, made from
/// the project's commit, its submodules filled from the project's own
/// checkouts of them. One a stopped run left is taken up again, with what
/// its agent committed there. ponytail: git runs on the UI thread; move
/// it off if a big checkout makes the editor stall.
fn add_worktree(project: &Path, started: u64, id: u32) -> Result<Worktree, String> {
    let (path, branch) = worktree_place(project, started, id)?;
    let mut base = git(project, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if base == "HEAD" {
        // Detached: its commit.
        base = git(project, &["rev-parse", "--short", "HEAD"])?;
    }
    let prefix = git(project, &["rev-parse", "--show-prefix"])?;
    if path.exists() {
        if git(&path, &["symbolic-ref", "--short", "HEAD"]).as_deref() != Ok(&branch) {
            return Err(format!(
                "{} is in the way of the agent's worktree.",
                path.display()
            ));
        }
    } else {
        // Forgets worktrees whose folders were deleted by hand.
        let _ = git(project, &["worktree", "prune"]);
        let at = path.to_string_lossy();
        let kept = format!("refs/heads/{branch}");
        // A kept branch the project already has all of is only in the
        // way of what merged since: start it again from here.
        let fresh = git(project, &["rev-parse", "--verify", "--quiet", &kept]).is_err()
            || git(project, &["merge-base", "--is-ancestor", &kept, "HEAD"]).is_ok();
        if fresh {
            git(project, &["worktree", "add", "-B", &branch, &at, "HEAD"])?;
        } else {
            git(project, &["worktree", "add", &at, &branch])?;
        }
    }
    // Again on a reused one, in case filling it failed before.
    fill_submodules(project, &path)?;
    let dir = path.join(prefix);
    Ok(Worktree {
        target: dir.join("target"),
        dir,
        path,
        branch,
        base,
    })
}

/// Checks out `worktree`'s submodules, which `git worktree add` leaves
/// empty, from the project's own checkouts of them, never their remotes:
/// one the project has not checked out stays empty there too. A CCGS
/// game links its `.claude` folders into one.
fn fill_submodules(project: &Path, worktree: &Path) -> Result<(), String> {
    let Ok(paths) = git(
        worktree,
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    ) else {
        // None.
        return Ok(());
    };
    let top = PathBuf::from(git(project, &["rev-parse", "--show-toplevel"])?);
    let mut args = vec!["-c".to_string(), "protocol.file.allow=always".into()];
    let mut local_paths = Vec::new();
    for line in paths.lines() {
        let Some((key, path)) = line.split_once(' ') else {
            continue;
        };
        let name = key
            .trim_start_matches("submodule.")
            .trim_end_matches(".path");
        let local = top.join(path);
        if local.join(".git").exists() {
            args.extend([
                "-c".into(),
                format!(
                    "submodule.{name}.url={}",
                    local.to_string_lossy().replace('\\', "/")
                ),
            ]);
            local_paths.push(path.to_string());
        }
    }
    if local_paths.is_empty() {
        return Ok(());
    }
    args.extend(["submodule", "update", "--init", "--recursive", "--"].map(String::from));
    args.extend(local_paths);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git(worktree, &args).map(|_| ())
}

/// Commits what `name`'s agent left in `worktree`, merges it into the
/// project's branch, and removes the worktree and its branch. A merge
/// that fails leaves the branch, so the work is not lost, and one the
/// user is in the middle of is left alone. Ok carries a note when the
/// work merged but the worktree could not go.
fn merge_back(project: &Path, worktree: &Worktree, name: &str) -> Result<Option<String>, String> {
    let keep = |why: String| {
        format!(
            "Could not merge {} into {}: {why}\nThe branch keeps the work; merge it by hand.",
            worktree.branch, worktree.base
        )
    };
    let merging =
        |dir: &Path| git(dir, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).is_ok();
    if merging(project) {
        return Err(keep("the project is in the middle of a merge.".into()));
    }
    git(&worktree.path, &["add", "-A"])?;
    if git(&worktree.path, &["diff", "--cached", "--quiet"]).is_err() {
        let message = format!("Keep what {name} left uncommitted");
        git(&worktree.path, &["commit", "-m", &message])?;
    }
    // What it did, even on a branch it switched to.
    let work = git(&worktree.path, &["rev-parse", "HEAD"])?;
    let at = worktree.path.to_string_lossy();
    let message = format!("Merge what {name} did beside the other agents");
    if let Err(error) = git(project, &["merge", "--no-edit", "-m", &message, &work]) {
        // Only a merge this one started; one git refused never began.
        if merging(project) {
            let _ = git(project, &["merge", "--abort"]);
        }
        // The branch holds it, even if the agent switched away. Git
        // refuses while the worktree has it out, at that commit already.
        let _ = git(project, &["branch", "-f", &worktree.branch, &work]);
        let _ = git(project, &["worktree", "remove", "--force", &at]);
        return Err(keep(error));
    }
    let removed = git(project, &["worktree", "remove", "--force", &at])
        .and_then(|_| git(project, &["branch", "-d", &worktree.branch]));
    Ok(removed.err().map(|error| {
        format!(
            "Merged into {}, but {} or {} is left: {error}",
            worktree.base,
            worktree.path.display(),
            worktree.branch
        )
    }))
}

/// Which of `ready` start now, and whether beside others, each in a
/// worktree of its own: none while an agent works in the project folder,
/// the first alone there when nothing else would run, or when the project
/// is not in git, and otherwise all of them.
fn starting(ready: &[u32], in_project: bool, others: usize, git: bool) -> (&[u32], bool) {
    if in_project || ready.is_empty() || (!git && others > 0) {
        (&[], false)
    } else if others == 0 && (ready.len() == 1 || !git) {
        (&ready[..1], false)
    } else {
        (ready, true)
    }
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
    /// When it first started, in seconds; 0 in files from before.
    started: u64,
    /// What the project folder held beyond its commit when it first
    /// started (see `uncommitted`); 0 in files from before.
    untouched: u64,
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
    /// The roles tree on the left, kept in `tree_file`.
    tree: roles_tree::Tree,
    tree_file: PathBuf,
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
    /// Where the editor's game stands: only once it runs, built from the
    /// project folder, does the project's target folder hold what worktrees
    /// fill theirs from.
    game: GameBuild,
    /// The target folder the project builds in, looked up when the run
    /// began, which worktrees fill theirs from.
    build: Option<PathBuf>,
    /// The run said why its agents went one at a time for now.
    held_back: bool,
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
        let tree_file = dir.with_file_name(TREE_FILE);
        Self {
            org: Organization::read(&dir.join(format!("{name}.besfa"))),
            tree: read_ron(&tree_file).unwrap_or_default(),
            tree_file,
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
            game: GameBuild::Missing,
            build: None,
            held_back: false,
            refused: None,
            markdown: Default::default(),
        }
    }

    /// Where the editor's game stands: agents work side by side in
    /// worktrees only while it runs, built from the project folder.
    pub fn set_game(&mut self, game: GameBuild) {
        self.game = game;
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
            started: run.started,
            untouched: run.untouched,
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

    /// Each organization that has agents, with them in run order and what
    /// each does, one organization a line, for a chat agent to choose from.
    /// Empty when no organization has agents.
    pub fn lineup(&self) -> String {
        let mut lineup = String::new();
        for name in &self.names {
            let read;
            let org = if *name == self.name {
                &self.org
            } else {
                read = Organization::read(&self.file(name));
                &read
            };
            let agents: Vec<String> = org
                .order()
                .unwrap_or_default()
                .iter()
                .filter_map(|&id| org.node(id))
                .map(|node| {
                    let does: String = node
                        .instructions
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(100)
                        .collect();
                    if does.trim().is_empty() {
                        node.name.clone()
                    } else {
                        format!("{} ({})", node.name, does.trim())
                    }
                })
                .collect();
            // The count, since each agent is a run of its own to pay for.
            if !agents.is_empty() {
                let count = match agents.len() {
                    1 => "1 agent".to_string(),
                    count => format!("{count} agents"),
                };
                lineup.push_str(&format!("- {name} ({count}): {}\n", agents.join("; ")));
            }
        }
        lineup
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
        // A repository without a commit has nothing to make worktrees from.
        let git = git(directory, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
        // Only one order goes through every link when each agent takes
        // from the one before: a chain, no one beside another.
        let parallel = git
            && order
                .windows(2)
                .any(|pair| !self.org.links.contains(&(pair[0], pair[1])));
        let started = match run.started {
            0 => SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |since| since.as_secs()),
            started => started,
        };
        // A new run takes the place of a paused one.
        self.paused = None;
        self.held_back = false;
        self.build = Some(target::project_target(directory));
        self.run = Some(Run {
            organization: run.organization,
            task: run.task,
            files: run.files,
            chat: run.chat,
            order,
            invocations,
            round: run.round.max(1),
            running: Vec::new(),
            answers,
            stopped: false,
            halted: false,
            directory: directory.into(),
            watched: (watch.clone(), modified(&watch)),
            merge: access.github && access.merge,
            git,
            parallel,
            started,
            // From when it first started: a resume must still see what
            // the agents left since.
            untouched: match run.untouched {
                0 => uncommitted(directory),
                untouched => untouched,
            },
            one_at_a_time: false,
        });
        Ok(())
    }

    /// Sends `line`, the answer to what an agent asked, to that agent.
    pub fn send(&mut self, line: &str) {
        let Some(run) = &mut self.run else {
            return;
        };
        let id = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|answer| answer["response"]["request_id"].as_str().map(String::from));
        let Some(id) = id else {
            return;
        };
        if let Some(running) = run
            .running
            .iter_mut()
            .find(|running| running.asked.contains(&id))
        {
            running.asked.retain(|asked| *asked != id);
            if let Some(process) = &mut running.process {
                process.send(line);
            }
        }
    }

    /// Lets the Claude agents still to start in this run use what `rule`,
    /// one of Claude's allow rules, allows. They were set up when the run
    /// began, before the user allowed it.
    pub fn allow(&mut self, rule: &str) {
        let Some(run) = &mut self.run else {
            return;
        };
        for (program, args, _) in &mut run.invocations {
            if program != Provider::Claude.command() {
                continue;
            }
            // The list takes every value up to the next flag.
            match args.iter().position(|arg| arg == "--allowedTools") {
                Some(at) => args.insert(at + 1, rule.into()),
                None => args.extend(["--allowedTools".into(), rule.into()]),
            }
        }
    }

    /// Stops the agents at work, and keeps more from starting.
    pub fn stop(&mut self) {
        if let Some(run) = &mut self.run {
            run.stopped = true;
            for process in run
                .running
                .iter()
                .filter_map(|running| running.process.as_ref())
            {
                process.stop();
            }
        }
    }

    /// Starts the agents whose earlier agents are done, each answering in
    /// a message of its own added to `chat`. Once the last one is done,
    /// or one failed and the rest at work finished, returns whether the
    /// run changed the watched file. What the agents ask the user goes to
    /// `asks`.
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        (chat, asks): (&mut Vec<Message>, &mut Vec<Ask>),
    ) -> Option<bool> {
        let run = self.run.as_mut()?;
        if !run.stopped && !run.halted {
            let ready = self.org.ready(&run.order);
            let in_project = run.running.iter().any(|running| running.worktree.is_none());
            let mut git = run.git && !run.one_at_a_time;
            // Worktrees build in target folders filled from the project's,
            // which holds the build only once the editor has built the game
            // as the last commit has it; otherwise each worktree would build
            // the game's dependencies from scratch. While the game is being
            // built, after an agent's code merged say, those that would go
            // into worktrees wait; with no game, they go one at a time.
            let mut waiting = false;
            if starting(&ready, in_project, run.running.len(), git).1 {
                let manifests = ["diff", "--quiet", "HEAD", "--", "Cargo.toml", "Cargo.lock"];
                let why = match self.game {
                    GameBuild::Building => {
                        waiting = true;
                        None
                    }
                    GameBuild::Missing => Some(t(
                        "The agents go one at a time in the project folder until the editor has built the game: in worktrees each would build its dependencies from scratch.",
                    )),
                    GameBuild::Built if self::git(&run.directory, &manifests).is_err() => Some(t(
                        "The agents go one at a time in the project folder while Cargo.toml or Cargo.lock has uncommitted changes: worktrees made from the last commit would build the dependencies from scratch.",
                    )),
                    GameBuild::Built => None,
                };
                if let Some(why) = why {
                    git = false;
                    if !self.held_back {
                        self.held_back = true;
                        chat.push(Message {
                            speaker: Some((run.organization.clone(), [0x8A; 3])),
                            text: why.into(),
                            ..Default::default()
                        });
                    }
                }
            }
            if !waiting
                && starting(&ready, in_project, run.running.len(), git).1
                && uncommitted(&run.directory) != run.untouched
            {
                run.one_at_a_time = true;
                git = false;
                chat.push(Message {
                    speaker: Some((run.organization.clone(), [0x8A; 3])),
                    text: t("The project folder has uncommitted changes since the run began, so the agents go one at a time there.").into(),
                    ..Default::default()
                });
            }
            let (now, beside) = starting(&ready, in_project, run.running.len(), git);
            let now = if waiting { Vec::new() } else { now.to_vec() };
            if waiting && run.running.is_empty() {
                // Nothing to poll meanwhile; the game's build output wakes
                // the editor, but not always often.
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
                return None;
            }
            for &id in &now {
                self.start_agent(id, beside, ctx, chat);
                // One that could not start keeps the rest from starting.
                if self.run.as_ref().is_some_and(|run| run.halted) {
                    break;
                }
            }
            if !now.is_empty() {
                // Closing the editor now kills the agents; they run again.
                self.record(chat);
            }
        }
        let run = self.run.as_mut()?;
        if run.running.is_empty() {
            if !run.stopped && !run.halted && self.next_round(chat) {
                self.record(chat);
                ctx.request_repaint();
                return None;
            }
            return self.finish(chat);
        }
        let mut changed = false;
        let mut index = 0;
        while index < run.running.len() {
            let id = run.running[index].id;
            let node = self.org.node_mut(id).expect("running nodes exist");
            let outputs = run.running[index].poll(run.stopped, ctx, node);
            if outputs.is_empty() {
                index += 1;
                continue;
            }
            let mut exit = None;
            for output in outputs {
                match output {
                    Output::Line(line) => match Ask::read(&line) {
                        Some(ask) => {
                            run.running[index].asked.push(ask.id().into());
                            asks.push(ask);
                        }
                        None => {
                            if agent::ended(&line)
                                && let Some(process) = &mut run.running[index].process
                            {
                                process.close();
                            }
                            node.take(&line);
                        }
                    },
                    Output::Exit(code) => {
                        exit = Some(code);
                        break;
                    }
                }
            }
            let message = run.running[index].message;
            if let Some(code) = exit {
                let running = run.running.remove(index);
                // Nobody is left to take its answers.
                asks.retain(|ask| !running.asked.iter().any(|asked| asked == ask.id()));
                let place = run.order.iter().position(|&other| other == id);
                let answer = place.and_then(|place| run.answers[place].clone());
                let text = answer
                    .as_ref()
                    .and_then(|file| fs::read_to_string(file).ok());
                if let Some(file) = &answer {
                    let _ = fs::remove_file(file);
                }
                // One that finished as Stop came keeps its answer.
                if code != 0 {
                    let why = if run.stopped {
                        "Stopped.".to_string()
                    } else {
                        format!("Exited with code {code}.")
                    };
                    push_line(&mut node.output, &why);
                    if let Some(worktree) = &running.worktree {
                        push_line(
                            &mut node.output,
                            &format!(
                                "Its work stays in {} on {}, for Continue.",
                                worktree.path.display(),
                                worktree.branch
                            ),
                        );
                    }
                    node.state = NodeState::Failed;
                    run.halted = true;
                } else {
                    if let Some(text) = text {
                        node.output = text;
                    }
                    node.state = NodeState::Done;
                    if let Some(worktree) = &running.worktree {
                        match merge_back(&run.directory, worktree, &node.name) {
                            Ok(None) => {}
                            Ok(Some(note)) => push_line(&mut node.output, &note),
                            Err(error) => {
                                push_line(&mut node.output, &error);
                                node.state = NodeState::Failed;
                                run.halted = true;
                            }
                        }
                    }
                }
                changed = true;
                ctx.request_repaint();
            } else {
                index += 1;
            }
            if let Some(message) = chat.get_mut(message) {
                show(node, message);
            }
        }
        if changed {
            self.record(chat);
        }
        None
    }

    /// Starts agent `id`, `beside` others in a worktree of its own or
    /// else in the project folder, in a new message of `chat`. One that
    /// cannot start fails, and halts the run.
    fn start_agent(&mut self, id: u32, beside: bool, ctx: &egui::Context, chat: &mut Vec<Message>) {
        let Some(run) = &mut self.run else {
            return;
        };
        let place = run
            .order
            .iter()
            .position(|&other| other == id)
            .expect("ready nodes are ordered");
        let node = self.org.node(id).expect("ordered nodes exist");
        let color = node.color();
        chat.push(Message {
            speaker: Some((node.name.clone(), [color.r(), color.g(), color.b()])),
            ..Default::default()
        });
        let message = chat.len() - 1;
        let (program, args, env) = run.invocations[place].clone();
        let worktree = if program.is_empty() {
            // Its CLI could not be reached; the reason is the one arg.
            Err(args.concat())
        } else if beside || (run.git && worktree_kept(&run.directory, run.started, id)) {
            // A stopped run's worktree goes on, even for one alone.
            add_worktree(&run.directory, run.started, id).map(Some)
        } else {
            Ok(None)
        };
        let worktree = match worktree {
            Ok(worktree) => worktree,
            Err(error) => {
                let node = self.org.node_mut(id).expect("ordered nodes exist");
                node.output = error;
                node.state = NodeState::Failed;
                show(node, &mut chat[message]);
                run.halted = true;
                return;
            }
        };
        let input = self.org.input(
            id,
            &run.task,
            run.round,
            run.merge,
            run.parallel,
            worktree.as_ref(),
        );
        let node = self.org.node_mut(id).expect("ordered nodes exist");
        node.state = NodeState::Running;
        let mut running = Running {
            id,
            process: None,
            waiting: None,
            message,
            worktree: None,
            asked: Vec::new(),
        };
        let directory = worktree
            .as_ref()
            .map_or_else(|| run.directory.clone(), |tree| tree.dir.clone());
        let mut env = env;
        let mut filling = None;
        if let Some(tree) = &worktree {
            env.push(("CARGO_TARGET_DIR".into(), tree.target.display().to_string()));
            // A stopped run's folder keeps what its agent built.
            if !tree.target.exists() {
                let (from, to, top) = (self.build.clone(), tree.target.clone(), tree.path.clone());
                let (sender, filled) = mpsc::channel();
                let ctx = ctx.clone();
                thread::spawn(move || {
                    let own = target::own_names(&top);
                    let _ = sender.send(target::seed(from.as_deref(), &to, &own));
                    ctx.request_repaint();
                });
                filling = Some(filled);
                node.thoughts = t("Filling its target folder from what the project built.").into();
            }
        }
        match filling {
            Some(filled) => {
                running.waiting = Some(Waiting {
                    filled,
                    directory,
                    invocation: (program, args, env),
                    input,
                });
                show(node, &mut chat[message]);
            }
            None => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let env: Vec<(&str, &str)> = env
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str()))
                    .collect();
                match CliProcess::start(&directory, &program, &args, &env, ctx) {
                    Ok(mut process) => {
                        agent::send_prompt(&mut process, &program, input);
                        running.process = Some(process);
                    }
                    Err(error) => {
                        node.output = format!("Could not start {program}: {error}");
                        node.state = NodeState::Failed;
                        show(node, &mut chat[message]);
                        run.halted = true;
                        return;
                    }
                }
            }
        }
        running.worktree = worktree;
        run.running.push(running);
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
        let tree_before = self.tree.clone();
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
        // The tree changes on a click, not on every frame.
        let tree_error = (self.tree != tree_before)
            .then(|| write_ron(&self.tree_file, &self.tree))
            .and_then(Result::err);
        // Not on every frame of a drag. A tree that would not write does not
        // keep the organization from being written.
        let org_error = if ui.input(|input| input.pointer.any_down()) {
            None
        } else {
            self.save().err()
        };
        tree_error.or(org_error)
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
            RichText::new(t("Claude Code Game Studios' agents. Double-click one to add it."))
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
            if ui
                .button(t("+ Folder"))
                .on_hover_text(t(
                    "Right-click a row to rename or delete it. An agent moves to another folder in its settings.",
                ))
                .clicked()
            {
                self.tree.new_folder(&[]);
            }
            ui.add_space(4.0);
            let pos = self.drop_point(ui);
            match self.tree.show(ui, running) {
                Some(roles_tree::Entry::Role(name)) => {
                    let id = self.org.add(&name, pos);
                    self.selected = vec![id];
                    self.group = None;
                }
                Some(roles_tree::Entry::Agent(saved)) => {
                    let id = self.org.add_saved(saved, pos);
                    self.selected = vec![id];
                    self.group = None;
                }
                _ => {}
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
            // CCGS roles are in the tree already.
            let own = self.org.node(id).filter(|node| node.role.is_empty());
            if let Some(node) = own
                && ui
                    .button(t("Add to roles tree"))
                    .on_hover_text(t("Keeps this agent in the tree on the left, to add again."))
                    .clicked()
            {
                self.tree.add_agent(roles_tree::Saved::of(node));
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
            ui.add_space(4.0);
            ui.label(RichText::new(t("Roles tree")).small().color(panels::MUTED));
            self.tree.folder_picker(ui, id, node);
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

/// Puts `node`'s answer and thoughts so far in its `message`.
fn show(node: &Node, message: &mut Message) {
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
        let input = org.input(code, "Add a jump.", 1, false, false, None);
        assert!(input.starts_with("You are Gameplay Programmer"));
        assert!(input.contains("Implements designed mechanics"));
        assert!(input.contains("Add a jump."));
        assert!(input.contains("## Game Designer\nJumping costs stamina."));
        assert!(input.contains("rather than asking whether you may"));
        assert!(!input.contains("gh pr merge"));
        assert!(
            !org.input(design, "Add a jump.", 1, false, false, None)
                .contains("before you reported")
        );
        let merging = org.input(code, "Add a jump.", 1, true, false, None);
        assert!(merging.contains("gh pr merge --merge"));
        assert!(merging.contains("Never use `--admin`"));
        assert!(merging.contains("gh issue close"));
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
            round: 1,
            running: Vec::new(),
            answers: vec![Some(answer.clone()), None, None],
            stopped: false,
            halted: false,
            directory: ".".into(),
            watched: ("none".into(), None),
            merge: false,
            git: false,
            parallel: false,
            started: 0,
            untouched: 0,
            one_at_a_time: false,
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
    fn reads_which_organization_a_chat_agent_hands_its_task_to() {
        let said = "Design, code and test a double jump.\r\n\r\n**ORGANIZATION:** Studio.\r\n";
        let brief = "Design, code and test a double jump.";
        assert_eq!(handoff(said), Some(("Studio".into(), brief.into())));
        assert_eq!(handoff("organization: `Art_Team`").unwrap().0, "Art_Team");
        for marked in [
            "**ORGANIZATION**: Studio",
            "`ORGANIZATION`: Studio",
            "__ORGANIZATION__: Studio",
            // As the note's template has it.
            "ORGANIZATION: <Studio>",
            "Brief.\n```\nORGANIZATION: Studio\n```\n",
        ] {
            assert_eq!(handoff(marked).unwrap().0, "Studio", "{marked:?}");
        }
        // The last line counts; the brief keeps the lines before it.
        let twice = "ORGANIZATION: Studio\nNo, better:\nORGANIZATION: QA team\n";
        let (name, brief) = handoff(twice).unwrap();
        assert_eq!(name, "QA team");
        assert_eq!(brief, "ORGANIZATION: Studio\nNo, better:");
        for none in [
            "I added the jump myself.",
            "The designer said:\n> ORGANIZATION: Studio",
            "ORGANIZATION:  **",
            "End with ORGANIZATION: <name> to hand it over.",
            // Quoted on the way, after the work was done.
            "Here is how:\n```\nORGANIZATION: Studio\n```\nI did it myself.",
            "",
        ] {
            assert_eq!(handoff(none), None, "{none:?}");
        }
    }

    #[test]
    fn lists_each_organization_with_agents_for_a_chat_agent_to_pick() {
        let besfa = std::env::temp_dir().join(format!("besfa-lineup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&besfa);
        let dir = besfa.join("organizations");
        fs::create_dir_all(&dir).unwrap();
        let (mut org, [design, code, _]) = studio();
        org.link(code, design);
        org.write(&dir.join("Studio.besfa")).unwrap();
        let mut art = Organization::default();
        let artist = art.add("", Pos2::ZERO);
        art.node_mut(artist).unwrap().name = "Painter".into();
        art.write(&dir.join("Art.besfa")).unwrap();
        // One with no agents has nothing to hand a task to.
        Organization::default()
            .write(&dir.join("Empty.besfa"))
            .unwrap();
        let mut panel = OrgPanel::new(dir);
        assert_eq!(panel.names, ["Art", "Empty", "Studio"]);
        // The shown one as it is now, saved or not.
        panel.org.node_mut(artist).unwrap().instructions = "Paints the UI.\nIn oils.".into();

        let lineup = panel.lineup();
        let lines: Vec<&str> = lineup.lines().collect();
        assert_eq!(lines.len(), 2, "{lineup}");
        assert_eq!(lines[0], "- Art (1 agent): Painter (Paints the UI.)");
        // In run order, each with what it does.
        assert!(
            lines[1].starts_with("- Studio (3 agents): Gameplay Programmer ("),
            "{lineup}"
        );
        assert!(lines[1].contains("; Game Designer ("), "{lineup}");
        assert!(lines[1].contains("; Qa Tester ("), "{lineup}");
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
            round: 1,
            running: Vec::new(),
            answers: vec![None, None, None],
            stopped: false,
            halted: false,
            directory: ".".into(),
            watched: ("none".into(), None),
            merge: false,
            git: false,
            parallel: false,
            started: 0,
            untouched: 0,
            one_at_a_time: false,
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
                .is_some_and(|run| run.round == 2 && !run.running.is_empty())
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
    fn starts_side_by_side_the_agents_whose_earlier_ones_are_done() {
        let (mut org, [design, code, test]) = studio();
        org.link(design, code);
        let order = org.order().unwrap();
        for node in &mut org.nodes {
            node.state = NodeState::Waiting;
        }
        // The designer and the tester wait on no one.
        let ready = org.ready(&order);
        assert_eq!(ready, [design, test]);
        assert_eq!(starting(&ready, false, 0, true), (&ready[..], true));
        // Without git, one at a time in the project folder.
        assert_eq!(starting(&ready, false, 0, false), (&ready[..1], false));
        assert_eq!(starting(&ready, false, 1, false), (&[][..], false));
        // One alone works in the project folder, and none start beside it.
        assert_eq!(starting(&ready[..1], false, 0, true), (&ready[..1], false));
        assert_eq!(starting(&ready, true, 1, true), (&[][..], false));
        // Beside agents in worktrees, even one goes in its own.
        assert_eq!(starting(&ready[..1], false, 2, true), (&ready[..1], true));

        org.node_mut(design).unwrap().state = NodeState::Running;
        assert_eq!(org.ready(&order), [test]);
        org.node_mut(design).unwrap().state = NodeState::Done;
        assert_eq!(org.ready(&order), [code, test]);
    }

    /// A git repository with one commit, in a folder of its own under
    /// the temp folder, so the worktrees next to it go there too.
    fn repository(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("besfa-org-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("game");
        fs::create_dir_all(&project).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Test"],
            &["config", "user.email", "test@example.com"],
        ] {
            git(&project, args).unwrap();
        }
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        git(&project, &["add", "-A"]).unwrap();
        git(&project, &["commit", "-q", "-m", "Start"]).unwrap();
        (root, project)
    }

    #[test]
    fn merges_an_agents_worktree_back_and_removes_it_and_its_branch() {
        let (root, project) = repository("worktree");
        let tree = add_worktree(&project, 5, 1).unwrap();
        assert_eq!(tree.base, "main");
        assert_eq!(tree.branch, "besfa-org/5-1");
        // One a stopped run left is taken up again as it was.
        fs::write(tree.dir.join("jump.rs"), "// jump\n").unwrap();
        let again = add_worktree(&project, 5, 1).unwrap();
        assert_eq!(again.path, tree.path);
        assert!(again.dir.join("jump.rs").is_file());

        assert!(worktree_kept(&project, 5, 1));
        assert!(!worktree_kept(&project, 5, 2));
        // What it left uncommitted comes along too.
        assert_eq!(merge_back(&project, &tree, "Gameplay Programmer"), Ok(None));
        assert!(!worktree_kept(&project, 5, 1));
        assert!(project.join("jump.rs").is_file());
        assert!(!tree.path.exists());
        assert!(
            git(
                &project,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/heads/besfa-org/5-1"
                ]
            )
            .is_err()
        );
        let log = git(&project, &["log", "--format=%s"]).unwrap();
        assert!(log.contains("Keep what Gameplay Programmer left uncommitted"));

        // Two that change the same line: the second's branch stays.
        let one = add_worktree(&project, 6, 1).unwrap();
        let two = add_worktree(&project, 6, 2).unwrap();
        fs::write(one.dir.join("main.rs"), "fn main() { one() }\n").unwrap();
        fs::write(two.dir.join("main.rs"), "fn main() { two() }\n").unwrap();
        assert_eq!(merge_back(&project, &one, "One"), Ok(None));
        let error = merge_back(&project, &two, "Two").unwrap_err();
        assert!(error.contains("besfa-org/6-2"), "{error}");
        assert!(!two.path.exists());
        assert!(
            git(
                &project,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "refs/heads/besfa-org/6-2"
                ]
            )
            .is_ok()
        );
        assert_eq!(
            fs::read_to_string(project.join("main.rs")).unwrap().trim(),
            "fn main() { one() }"
        );
        assert_eq!(git(&project, &["status", "--porcelain"]).unwrap(), "");
        // Continue takes the kept branch up again, with its work.
        assert!(worktree_kept(&project, 6, 2));
        let two = add_worktree(&project, 6, 2).unwrap();
        assert!(
            fs::read_to_string(two.dir.join("main.rs"))
                .unwrap()
                .contains("two()")
        );

        // The user's own merge in the middle is left alone.
        let first = git(&project, &["rev-list", "--max-parents=0", "HEAD"]).unwrap();
        git(&project, &["checkout", "-q", "-b", "mine", &first]).unwrap();
        fs::write(project.join("main.rs"), "fn main() { mine() }\n").unwrap();
        git(&project, &["commit", "-q", "-am", "Mine"]).unwrap();
        assert!(git(&project, &["merge", "main"]).is_err());
        let error = merge_back(&project, &two, "Two").unwrap_err();
        assert!(error.contains("middle of a merge"), "{error}");
        assert!(
            git(
                &project,
                &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]
            )
            .is_ok()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fills_a_worktrees_submodules_from_the_projects_own() {
        let (root, project) = repository("submodule");
        let (_, library) = repository("submodule-lib");
        fs::write(library.join("agent.md"), "# Agent\n").unwrap();
        git(&library, &["add", "-A"]).unwrap();
        git(&library, &["commit", "-q", "-m", "Agent"]).unwrap();
        let from = library.to_string_lossy().replace('\\', "/");
        git(
            &project,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                &from,
                ".ccg",
            ],
        )
        .unwrap();
        git(&project, &["commit", "-q", "-m", "CCGS"]).unwrap();
        // Its remote is gone; the project's own checkout fills it.
        fs::remove_dir_all(&library).unwrap();
        let tree = add_worktree(&project, 7, 1).unwrap();
        assert!(tree.dir.join(".ccg/agent.md").is_file());
        assert_eq!(merge_back(&project, &tree, "Agent"), Ok(None));
        assert!(!tree.path.exists());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(library.parent().unwrap());
    }

    #[test]
    fn runs_agents_side_by_side_in_worktrees_and_merges_their_work() {
        // Whether the designer commits its work: one that leaves it in the
        // project folder has the others go one at a time there.
        for commits in [true, false] {
            let (root, project) = repository(if commits { "side" } else { "side-left" });
            let (mut org, [design, code, test]) = studio();
            org.link(design, code);
            org.link(design, test);
            // In the repository, as in a game: the run file lands there.
            let mut panel = OrgPanel::new(project.join(ORGANIZATIONS_DIR));
            panel.org = org;
            panel.set_game(GameBuild::Built);
            for node in &mut panel.org.nodes {
                node.state = NodeState::Waiting;
            }
            // Each writes a file where it works and tells where cargo would
            // build and where that was.
            let write = |file: &str, commit: bool| -> Invocation {
                let commit = if commit {
                    "& git add -A& git commit -q -m d"
                } else {
                    ""
                };
                let line =
                    format!("echo {file}>{file}{commit}& echo target=%CARGO_TARGET_DIR%& cd");
                ("cmd".into(), vec!["/c".into(), line], vec![])
            };
            panel.run = Some(Run {
                organization: "Organization".into(),
                task: "Add a jump.".into(),
                files: Vec::new(),
                chat: 1,
                order: vec![design, code, test],
                invocations: vec![
                    write("design.md", commits),
                    write("code.rs", false),
                    write("test.rs", false),
                ],
                round: 1,
                running: Vec::new(),
                answers: vec![None, None, None],
                stopped: false,
                halted: false,
                directory: project.clone(),
                watched: ("none".into(), None),
                merge: false,
                git: true,
                parallel: true,
                started: 9,
                untouched: uncommitted(&project),
                one_at_a_time: false,
            });
            let ctx = egui::Context::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let (mut chat, mut finished) = (Vec::new(), None);
            while finished.is_none() && std::time::Instant::now() < deadline {
                finished = panel.poll(&ctx, (&mut chat, &mut Vec::new()));
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_eq!(finished, Some(false));
            let node = |id| panel.org.node(id).unwrap();
            assert!(
                [design, code, test]
                    .iter()
                    .all(|&id| node(id).state == NodeState::Done)
            );
            // The designer alone in the project; the other two, started
            // together, beside each other in worktrees.
            let game = project.display().to_string();
            // The last line is `cd`'s; git may warn before it.
            let in_project = |id| {
                let output = &node(id).output;
                output
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or_default()
                    .eq_ignore_ascii_case(&game)
            };
            assert!(in_project(design), "{}", node(design).output);
            for id in [code, test] {
                assert_eq!(in_project(id), !commits, "{}", node(id).output);
            }
            // One in a worktree builds in its own target folder; the one in
            // the project folder where the editor does.
            let own_target = |id| {
                let (tree, _) = worktree_place(&project, 9, id).unwrap();
                let target = format!("target={}", tree.join("target").display());
                node(id).output.lines().any(|line| line.trim() == target)
            };
            assert!(!own_target(design), "{}", node(design).output);
            for id in [code, test] {
                assert_eq!(own_target(id), commits, "{}", node(id).output);
            }
            for file in ["design.md", "code.rs", "test.rs"] {
                assert!(project.join(file).is_file(), "{file}");
            }
            // Their target folders stayed out of the commits merged back.
            let tracked = git(&project, &["ls-files"]).unwrap();
            assert!(
                !tracked.lines().any(|path| path.starts_with("target")),
                "{tracked}"
            );
            let said: Vec<&str> = chat
                .iter()
                .map(|message| message.speaker.as_ref().unwrap().0.as_str())
                .collect();
            if commits {
                assert_eq!(said, ["Game Designer", "Gameplay Programmer", "Qa Tester"]);
            } else {
                // The organization says why.
                assert_eq!(
                    said,
                    [
                        "Game Designer",
                        "Organization",
                        "Gameplay Programmer",
                        "Qa Tester"
                    ]
                );
            }
            // Nothing of theirs is left.
            assert_eq!(
                git(&project, &["worktree", "list"])
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
            assert_eq!(
                git(&project, &["branch", "--list", "besfa-org/*"]).unwrap(),
                ""
            );
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn keeps_agents_out_of_worktrees_until_the_project_build_can_fill_theirs() {
        // No game; built, with Cargo.lock not committed; being built, then
        // built while the designer's agents wait for it.
        for (game, lock) in [
            (GameBuild::Missing, false),
            (GameBuild::Built, true),
            (GameBuild::Building, false),
        ] {
            let (root, project) = repository(&format!("held-{game:?}"));
            if lock {
                fs::write(project.join("Cargo.lock"), "version = 4\n").unwrap();
                git(&project, &["add", "-A"]).unwrap();
                git(&project, &["commit", "-q", "-m", "Lock"]).unwrap();
                fs::write(project.join("Cargo.lock"), "version = 4\n# changed\n").unwrap();
            }
            let (mut org, [design, code, test]) = studio();
            org.link(design, code);
            org.link(design, test);
            let mut panel = OrgPanel::new(project.join(ORGANIZATIONS_DIR));
            panel.org = org;
            panel.set_game(game);
            for node in &mut panel.org.nodes {
                node.state = NodeState::Waiting;
            }
            let write = |file: &str| -> Invocation {
                let line = format!("echo {file}>{file}& git add {file}& git commit -q -m d& cd");
                ("cmd".into(), vec!["/c".into(), line], vec![])
            };
            panel.run = Some(Run {
                organization: "Organization".into(),
                task: "Add a jump.".into(),
                files: Vec::new(),
                chat: 1,
                order: vec![design, code, test],
                invocations: vec![write("design.md"), write("code.rs"), write("test.rs")],
                round: 1,
                running: Vec::new(),
                answers: vec![None, None, None],
                stopped: false,
                halted: false,
                directory: project.clone(),
                watched: ("none".into(), None),
                merge: false,
                git: true,
                parallel: true,
                started: 9,
                untouched: uncommitted(&project),
                one_at_a_time: false,
            });
            let ctx = egui::Context::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let (mut chat, mut finished) = (Vec::new(), None);
            let mut waited = false;
            while finished.is_none() && std::time::Instant::now() < deadline {
                finished = panel.poll(&ctx, (&mut chat, &mut Vec::new()));
                // Once the designer is done, the two after it wait for the
                // build, without the run ending; then it is built.
                let done = panel.org.node(design).unwrap().state == NodeState::Done;
                if game == GameBuild::Building && done && !waited {
                    for _ in 0..20 {
                        assert_eq!(panel.poll(&ctx, (&mut chat, &mut Vec::new())), None);
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    assert_eq!(chat.len(), 1);
                    waited = true;
                    panel.set_game(GameBuild::Built);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_eq!(finished, Some(false));
            let folder = project.display().to_string();
            let in_project = |id| {
                let output = &panel.org.node(id).unwrap().output;
                let last = output.trim().lines().last().unwrap_or_default();
                last.eq_ignore_ascii_case(&folder)
            };
            assert!(in_project(design));
            let said: Vec<&str> = chat
                .iter()
                .map(|message| message.speaker.as_ref().unwrap().0.as_str())
                .collect();
            if game == GameBuild::Building {
                // Built after all: side by side, with nothing to say.
                assert!(waited);
                assert!(!in_project(code) && !in_project(test));
                assert_eq!(said, ["Game Designer", "Gameplay Programmer", "Qa Tester"]);
            } else {
                // Said once, as the two after the designer were held back.
                assert!(in_project(code) && in_project(test));
                assert_eq!(
                    said,
                    [
                        "Game Designer",
                        "Organization",
                        "Gameplay Programmer",
                        "Qa Tester"
                    ]
                );
                let why = if lock { "Cargo.lock" } else { "built the game" };
                assert!(chat[1].text.contains(why), "{}", chat[1].text);
            }
            let _ = fs::remove_dir_all(&root);
        }
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
            round: 1,
            running: Vec::new(),
            answers: vec![None, None],
            stopped: false,
            halted: false,
            directory: ".".into(),
            watched: ("none".into(), None),
            merge: false,
            git: false,
            parallel: false,
            started: 0,
            untouched: 0,
            one_at_a_time: false,
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
    fn lets_the_agents_still_to_start_use_what_the_user_allowed() {
        let (org, [design, code, _]) = studio();
        let mut panel = OrgPanel::new(std::env::temp_dir().join("besfa-org-allow/organizations"));
        panel.org = org;
        let claude = |args: &[&str]| -> Invocation {
            let args = args.iter().map(|arg| arg.to_string()).collect();
            ("claude".into(), args, vec![])
        };
        panel.run = Some(Run {
            organization: "Organization".into(),
            task: "Add a jump.".into(),
            files: Vec::new(),
            chat: 1,
            order: vec![design, code],
            invocations: vec![
                claude(&["-p", "--allowedTools", "mcp__besfa", "--model", "opus"]),
                claude(&["-p", "--agent", "qa"]),
                ("codex".into(), vec!["exec".into()], vec![]),
                (String::new(), vec!["Gemini: log in first".into()], vec![]),
            ],
            round: 1,
            running: Vec::new(),
            answers: vec![None, None],
            stopped: false,
            halted: false,
            directory: ".".into(),
            watched: ("none".into(), None),
            merge: false,
            git: false,
            parallel: false,
            started: 0,
            untouched: 0,
            one_at_a_time: false,
        });
        panel.allow("Bash(cargo *)");
        let run = panel.run.take().unwrap();
        let args: Vec<Vec<String>> = run.invocations.iter().map(|i| i.1.clone()).collect();
        assert_eq!(
            args[0],
            [
                "-p",
                "--allowedTools",
                "Bash(cargo *)",
                "mcp__besfa",
                "--model",
                "opus"
            ]
        );
        assert_eq!(
            args[1],
            ["-p", "--agent", "qa", "--allowedTools", "Bash(cargo *)"]
        );
        assert_eq!(args[2], ["exec"]);
        assert_eq!(args[3], ["Gemini: log in first"]);
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
            .with_step_dt(1.0 / 60.0)
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

        // egui counts a click soon after two others as a triple click, which
        // is no double click; let the one above go by first.
        harness.run_steps(40);
        // One click adds nothing; the second of a double click does.
        harness.get_by_label("Producer").click();
        harness.step();
        assert_eq!(harness.state().org.nodes.len(), 4);
        harness.get_by_label("Producer").click();
        harness.step();
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
    fn a_saved_agent_shows_by_name_and_the_tree_is_kept() {
        use egui_kittest::{Harness, kittest::Queryable};
        let base = std::env::temp_dir().join(format!("besfa-tree-ui-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mut panel = OrgPanel::new(base.join("organizations"));
        panel.tree.add_agent(roles_tree::Saved {
            name: "UI Designer".into(),
            ..Default::default()
        });
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1200.0, 700.0))
            .build_ui_state(|ui, panel: &mut OrgPanel| drop(panel.ui(ui)), panel);
        harness.run();
        harness.get_by_label("UI Designer");
        // Closing a folder is an edit of the tree, which is written.
        harness.get_by_label("▾ My agents").click();
        harness.run();
        harness.get_by_label("▸ My agents");
        assert!(harness.query_by_label("UI Designer").is_none());
        let tree = harness.state().tree.clone();
        assert_eq!(
            read_ron::<roles_tree::Tree>(&base.join("roles.ron")),
            Some(tree)
        );
        fs::remove_dir_all(&base).unwrap();
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
