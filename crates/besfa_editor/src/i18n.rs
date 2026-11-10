//! The editor's languages: English as the code writes it, and Korean looked
//! up by that English text. Text missing from the table stays English.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use eframe::egui::Ui;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    Korean,
}

/// The language every `t` call reads; one editor shows one language.
static KOREAN: AtomicBool = AtomicBool::new(false);

impl Language {
    const ALL: [Self; 2] = [Self::English, Self::Korean];

    /// Each language named in itself, so anyone can find theirs.
    fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Korean => "한국어",
        }
    }

    pub fn current() -> Self {
        if KOREAN.load(Ordering::Relaxed) {
            Self::Korean
        } else {
            Self::English
        }
    }

    fn set(self) {
        KOREAN.store(self == Self::Korean, Ordering::Relaxed);
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
}

impl Settings {
    /// The settings file, next to the recent projects list.
    fn file() -> PathBuf {
        let base = std::env::var("APPDATA").or_else(|_| std::env::var("HOME"));
        Path::new(&base.unwrap_or_else(|_| ".".into())).join("Besfa/settings.ron")
    }

    fn read(file: &Path) -> Self {
        fs::read_to_string(file)
            .ok()
            .and_then(|text| ron::from_str::<Self>(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, file: &Path) -> Result<(), String> {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let text = ron::to_string(self).map_err(|error| error.to_string())?;
        fs::write(file, text).map_err(|error| error.to_string())
    }
}

/// Shows the editor in the language the settings file keeps.
pub fn load() {
    Settings::read(&Settings::file()).language.set();
}

/// The Settings menu: picks the language and keeps it, then shows `more`.
/// Returns why the language could not be kept.
pub fn settings_menu(ui: &mut Ui, more: impl FnOnce(&mut Ui)) -> Option<String> {
    let mut error = None;
    ui.menu_button(t("Settings"), |ui| {
        ui.menu_button(t("Language"), |ui| {
            for language in Language::ALL {
                let current = Language::current() == language;
                if ui.radio(current, language.label()).clicked() && !current {
                    language.set();
                    error = Settings { language }.write(&Settings::file()).err();
                    ui.close();
                }
            }
        });
        more(ui);
    });
    error
}

/// `en` in the editor's language.
pub fn t(en: &str) -> &str {
    match Language::current() {
        Language::English => en,
        Language::Korean => korean(en).unwrap_or(en),
    }
}

fn korean(en: &str) -> Option<&'static str> {
    Some(match en {
        // Menus
        "Settings" => "설정",
        "Language" => "언어",
        "Agent Settings..." => "에이전트 설정...",
        "Could not save the settings: {}" => "설정을 저장하지 못했습니다: {}",
        "File" => "파일",
        "Save scene" => "씬 저장",
        "Back to Project Hub" => "프로젝트 허브로 돌아가기",
        "Edit" => "편집",
        "Undo" => "실행 취소",
        "Redo" => "다시 실행",
        "Drop to surface" => "표면에 내려놓기",
        "Window" => "창",
        "Animation State Machine" => "애니메이션 상태 머신",
        "Agent" => "에이전트",
        "Agent Window" => "에이전트 창",
        "Help" => "도움말",
        "Bevy Assets" => "Bevy 에셋",
        // Toolbar and panels
        "Move (W)" => "이동 (W)",
        "Rotate (E)" => "회전 (E)",
        "Scale (R)" => "크기 (R)",
        "⏹ Stop" => "⏹ 중지",
        "▶ Play" => "▶ 재생",
        "Ctrl+click restarts the game" => "Ctrl+클릭하면 게임을 다시 시작합니다",
        "Standalone" => "단독 실행",
        "Server + 1 client" => "서버 + 클라이언트 1개",
        "Server + {} clients" => "서버 + 클라이언트 {}개",
        "Snap (hold Ctrl)" => "스냅 (Ctrl 누른 채)",
        "Grid" => "격자",
        "{} Click to update it to the latest commit." => {
            "{} 클릭하면 최신 커밋으로 업데이트합니다."
        }
        "The plugin differs from the Bevy prebuild. Click to update it to the latest commit." => {
            "플러그인이 Bevy 프리빌드와 다릅니다. 클릭하면 최신 커밋으로 업데이트합니다."
        }
        "The plugin matches the Bevy prebuild. Click to update it to the latest commit." => {
            "플러그인이 Bevy 프리빌드와 일치합니다. 클릭하면 최신 커밋으로 업데이트합니다."
        }
        "Log" => "로그",
        "Assets" => "에셋",
        "Inspector" => "인스펙터",
        "Scatter" => "흩뿌리기",
        "Animation" => "애니메이션",
        "Scene" => "씬",
        "Close the preview" => "미리보기 닫기",
        "Viewport" => "뷰포트",
        "Drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  F fit" => {
            "드래그 회전  ·  가운데 드래그 이동  ·  스크롤 확대/축소  ·  F 맞추기"
        }
        "Paint to place; hold Shift to erase.  ·  Right-drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  Esc stop" => {
            "칠해서 배치, Shift로 지우기  ·  오른쪽 드래그 회전  ·  가운데 드래그 이동  ·  스크롤 확대/축소  ·  Esc 중지"
        }
        "Paint to sculpt; hold Shift to invert.  ·  Right-drag orbit  ·  Middle-drag pan  ·  Scroll zoom  ·  Esc stop" => {
            "칠해서 조각, Shift로 반전  ·  오른쪽 드래그 회전  ·  가운데 드래그 이동  ·  스크롤 확대/축소  ·  Esc 중지"
        }
        "Click select  ·  Right-drag orbit  ·  Right-hold + WASD/QE fly  ·  Middle-drag pan  ·  Scroll zoom  ·  F focus" => {
            "클릭 선택  ·  오른쪽 드래그 회전  ·  오른쪽 누른 채 WASD/QE 비행  ·  가운데 드래그 이동  ·  스크롤 확대/축소  ·  F 초점"
        }
        "Hierarchy" => "계층",
        "Save as prefab" => "프리팹으로 저장",
        "Delete (Delete)" => "삭제 (Delete)",
        "Duplicate (Ctrl+D)" => "복제 (Ctrl+D)",
        "Add entity" => "엔티티 추가",
        "Empty" => "빈 엔티티",
        "Cube" => "큐브",
        "Sphere" => "구",
        "Plane" => "평면",
        "Light" => "조명",
        "Camera" => "카메라",
        "Terrain" => "지형",
        "Sky" => "하늘",
        "Fold" => "접기",
        "Unfold" => "펼치기",
        "Reimporting" => "다시 가져오는 중",
        "Reimported" => "다시 가져옴",
        "No assets folder" => "assets 폴더 없음",
        "Double-click to preview" => "더블 클릭하면 미리 봅니다",
        "Add to scene" => "씬에 추가",
        // Dialogs
        "Prefab name" => "프리팹 이름",
        "Saved to assets/prefabs" => "assets/prefabs에 저장됩니다",
        "Letters, digits, spaces, - and _ only" => "문자, 숫자, 공백, -, _만 쓸 수 있습니다",
        "Cancel" => "취소",
        "Save" => "저장",
        "Discard" => "저장 안 함",
        "Delete" => "삭제",
        "Save changes to the scene?" => "씬의 변경 사항을 저장할까요?",
        "The scene has changes that are not saved to scenes/main.scn.ron." => {
            "scenes/main.scn.ron에 저장하지 않은 변경 사항이 있습니다."
        }
        // Inspector
        "Hide details" => "세부 정보 숨기기",
        "Show details" => "세부 정보 보기",
        "Select an entity in the Hierarchy" => "계층에서 엔티티를 선택하세요",
        "Entity {} is gone" => "엔티티 {}이(가) 없습니다",
        "Components" => "컴포넌트",
        "Systems" => "시스템",
        "➕ Add component" => "➕ 컴포넌트 추가",
        "Add component" => "컴포넌트 추가",
        "Search components" => "컴포넌트 검색",
        "Part of {}; changes to it are saved on the placed model." => {
            "{}의 일부입니다. 변경 사항은 배치된 모델에 저장됩니다."
        }
        "Part of {}; Save scene skips it." => "{}의 일부입니다. 씬 저장에서 제외됩니다.",
        "Spawned by the game while it runs; Save scene skips it." => {
            "게임이 실행 중에 생성했습니다. 씬 저장에서 제외됩니다."
        }
        "⮉ Apply to prefab" => "⮉ 프리팹에 적용",
        "required by {}" => "{}에 필요",
        "computed" => "계산됨",
        "immutable" => "변경 불가",
        "no Reflect" => "Reflect 없음",
        "Remove component" => "컴포넌트 제거",
        "Give it a value" => "값 지정",
        "Make it None" => "None으로 만들기",
        "Add an item" => "항목 추가",
        "Remove this item" => "이 항목 제거",
        "key" => "키",
        "Add an entry by this key" => "이 키로 항목 추가",
        "Remove this entry" => "이 항목 제거",
        // Animation
        "Select a placed model in the Hierarchy." => "계층에서 배치된 모델을 선택하세요.",
        "Update besfa_editor_plugin in the game to play animation state machines." => {
            "애니메이션 상태 머신을 재생하려면 게임의 besfa_editor_plugin을 업데이트하세요."
        }
        "Put a state machine on a placed glTF model to play its clips." => {
            "배치된 glTF 모델에 상태 머신을 붙여 클립을 재생하세요."
        }
        "➕ Add state machine" => "➕ 상태 머신 추가",
        "This state machine has a shape the editor does not know." => {
            "에디터가 알지 못하는 형태의 상태 머신입니다."
        }
        "Playing" => "재생 중",
        "{} pulled" => "{} 발동",
        "States" => "상태",
        "The model's clips show once it has loaded." => "모델이 로드되면 클립이 표시됩니다.",
        "Plays first" => "처음 재생됨",
        "Play first" => "처음 재생",
        "Remove" => "제거",
        "Name " => "이름 ",
        "The model's clip it plays" => "재생할 모델 클립",
        "Once" => "한 번",
        "Plays once and holds its last pose" => "한 번 재생하고 마지막 포즈를 유지합니다",
        "➕ Add state" => "➕ 상태 추가",
        "Transitions" => "전환",
        "Taken first in the list first, once every condition holds." => {
            "모든 조건이 충족되면 목록에서 위에 있는 것부터 전환합니다."
        }
        "Any" => "모든 상태",
        "Any state but the one it goes to" => "도착 상태를 뺀 모든 상태",
        "Blend s " => "블렌드(초) ",
        "Crossfade seconds" => "크로스페이드 시간(초)",
        "Exit " => "종료 시점 ",
        "How much of the clip plays first, 1 for once through; 0 for any time" => {
            "먼저 재생할 클립의 비율: 1이면 끝까지 한 번, 0이면 언제든지"
        }
        "➕ Add transition" => "➕ 전환 추가",
        "➕ Condition" => "➕ 조건",
        // Scatter and terrain
        "Drag prefabs or models here to scatter them." => {
            "프리팹이나 모델을 여기로 드래그해 흩뿌리세요."
        }
        "How often it comes up against the others" => "다른 항목에 비해 나오는 빈도",
        "Brush size" => "브러시 크기",
        "Density" => "밀도",
        "Things per 100 m² under the brush" => "브러시 아래 100 m²당 개수",
        "Spacing" => "간격",
        "to" => "~",
        "Scale" => "크기",
        "Random rotation" => "무작위 회전",
        "Align to surface" => "표면에 맞춤",
        "Paint to place; hold Shift to erase." => "칠해서 배치합니다. Shift를 누르면 지웁니다.",
        "Raise" => "올리기",
        "Lower" => "내리기",
        "Smooth" => "부드럽게",
        "Flatten" => "평평하게",
        "Paint" => "칠하기",
        "Update besfa_editor_plugin in the game to sculpt terrain." => {
            "지형을 조각하려면 게임의 besfa_editor_plugin을 업데이트하세요."
        }
        "Strength" => "세기",
        "Raise and Lower: up to half the brush size in meters per stroke" => {
            "올리기/내리기: 한 번에 브러시 크기의 절반(미터)까지"
        }
        "Paint to sculpt; hold Shift to invert." => {
            "칠해서 조각합니다. Shift를 누르면 반대로 합니다."
        }
        "Layers" => "레이어",
        "Set as another color space in the file" => "파일에는 다른 색 공간으로 지정되어 있습니다",
        "Layer {}" => "레이어 {}",
        "➕ Add layer" => "➕ 레이어 추가",
        "A terrain has at most 4 layers." => "지형의 레이어는 최대 4개입니다.",
        // Model preview
        "Front" => "앞",
        "Back" => "뒤",
        "Left" => "왼쪽",
        "Right" => "오른쪽",
        "Top" => "위",
        "Look at the model's front, its -Z side" => "모델의 앞(-Z 쪽)을 봅니다",
        "Look at the model's back" => "모델의 뒤를 봅니다",
        "Look at the model's left side" => "모델의 왼쪽을 봅니다",
        "Look at the model's right side, its +X side" => "모델의 오른쪽(+X 쪽)을 봅니다",
        "Look down on the model" => "모델을 위에서 내려다봅니다",
        "Look at the model's front from above and to its right" => {
            "모델의 앞을 오른쪽 위에서 봅니다"
        }
        "Studio" => "스튜디오",
        "Daylight" => "낮",
        "Night" => "밤",
        "Model" => "모델",
        "Show all parts" => "모든 부분 보기",
        "Loading" => "로드 중",
        "Parts" => "부분",
        "Selected" => "선택됨",
        "Position" => "위치",
        "Rotation" => "회전",
        "Poses" => "포즈",
        "Rest" => "기본 자세",
        "Put every part back the way the model loaded" => "모든 부분을 로드된 상태로 되돌립니다",
        "Pose name" => "포즈 이름",
        "Save pose" => "포즈 저장",
        "Keep the parts' changed transforms under this name" => {
            "부분들의 바뀐 트랜스폼을 이 이름으로 저장합니다"
        }
        "Pose the model" => "모델에 포즈 적용",
        "Forget the pose" => "포즈 삭제",
        "{} parts" => "부분 {}개",
        "Revert 1 change" => "변경 1개 되돌리기",
        "Revert {} changes" => "변경 {}개 되돌리기",
        "Take back every change the panel made to the model" => {
            "이 패널이 모델에 한 변경을 모두 되돌립니다"
        }
        "No clips" => "클립 없음",
        "None (rest pose)" => "없음 (기본 자세)",
        "The model as it loaded, to pose by hand." => {
            "로드된 그대로의 모델입니다. 직접 포즈를 잡으세요."
        }
        "Pause" => "일시 정지",
        "Play" => "재생",
        "Back to the start" => "처음으로",
        "Loop" => "반복",
        "View" => "보기",
        "📷 Screenshot" => "📷 스크린샷",
        "Save what the viewport shows to {}/" => "뷰포트 화면을 {}/에 저장합니다",
        "Open the screenshots folder" => "스크린샷 폴더 열기",
        "Overlays" => "오버레이",
        "Skeleton ({})" => "뼈대 ({})",
        "Markers ({})" => "마커 ({})",
        "Parts that mark a place: leaves without a mesh, like Grip or ShotPoint" => {
            "위치를 표시하는 부분: Grip이나 ShotPoint처럼 메시가 없는 말단"
        }
        "Bounds" => "경계",
        "Marker positions" => "마커 위치",
        "The entity the model spawns under: add the game's components here" => {
            "모델이 생성되는 부모 엔티티: 게임의 컴포넌트는 여기에 추가하세요"
        }
        "No parts" => "부분 없음",
        "Show" => "보이기",
        "Hide" => "숨기기",
        "Show only this part" => "이 부분만 보기",
        "Show its components and pose it" => "컴포넌트를 보고 포즈를 잡습니다",
        "marker" => "마커",
        "joint" => "관절",
        "Materials ({})" => "머티리얼 ({})",
        "Unnamed" => "이름 없음",
        "{triangles} triangles · {vertices} vertices · {meshes} mesh{s}" => {
            "삼각형 {triangles}개 · 정점 {vertices}개 · 메시 {meshes}개"
        }
        " · {} joints" => " · 관절 {}개",
        "{meshes} mesh{s}" => "메시 {meshes}개",
        // Agents
        "Agent Settings" => "에이전트 설정",
        "API key" => "API 키",
        "checking" => "확인 중",
        "ready" => "준비됨",
        "can't send yet" => "아직 보낼 수 없음",
        "Keys stay in the Windows Credential Manager." => {
            "키는 Windows 자격 증명 관리자에 보관됩니다."
        }
        "Check again" => "다시 확인",
        "Checking `{}`." => "`{}` 확인 중입니다.",
        "`{}` is not installed." => "`{}`이(가) 설치되어 있지 않습니다.",
        "`{}` is not logged in. Run `{}` in a terminal to log in." => {
            "`{}`에 로그인되어 있지 않습니다. 터미널에서 `{}`을(를) 실행해 로그인하세요."
        }
        "`{}` is logged in." => "`{}`에 로그인되어 있습니다.",
        "{} API key" => "{} API 키",
        "Checking the command-line tool." => "명령줄 도구를 확인하는 중입니다.",
        "Install its command-line tool first." => "먼저 명령줄 도구를 설치하세요.",
        "Log in to its command-line tool, or switch it to an API key in Settings > Agent Settings." => {
            "명령줄 도구에 로그인하거나, 설정 > 에이전트 설정에서 API 키로 바꾸세요."
        }
        "Enter its API key in Settings > Agent Settings." => {
            "설정 > 에이전트 설정에서 API 키를 입력하세요."
        }
        "New chat" => "새 채팅",
        "Show history" => "기록 보기",
        "Hide history" => "기록 숨기기",
        "+ New" => "+ 새로 만들기",
        "+ New Agent" => "+ 새 에이전트",
        "History" => "기록",
        "No earlier chats." => "이전 채팅이 없습니다.",
        "Ask the agent to change the project." => "에이전트에게 프로젝트 변경을 요청하세요.",
        "Ask the agent to change the project. Ctrl+Enter sends." => {
            "에이전트에게 프로젝트 변경을 요청하세요. Ctrl+Enter로 보냅니다."
        }
        "+ Attach" => "+ 첨부",
        "Stop" => "중지",
        "Send" => "보내기",
        "CLI default" => "CLI 기본값",
        "Chat" => "채팅",
        "Organization" => "조직",
        "Agents with roles that work on a task together." => {
            "역할을 맡은 에이전트들이 함께 작업합니다."
        }
        "How each provider is reached, and what the agents may use in this project." => {
            "제공자마다 연결하는 방법과, 이 프로젝트에서 에이전트가 쓸 수 있는 것"
        }
        "Let Claude commit, push and open pull requests" => {
            "Claude가 커밋하고 푸시하고 풀 리퀘스트를 열게 하기"
        }
        "It then runs git and gh without asking, and gh reaches every repository your GitHub login can. Deny rules in the project's .claude/settings.json still win, but only over commands they match as written, so block force pushes with branch protection on GitHub." => {
            "그러면 Claude가 git과 gh를 묻지 않고 실행하고, gh는 GitHub 로그인으로 접근할 수 있는 모든 저장소를 다룹니다. 프로젝트 .claude/settings.json의 거부 규칙은 여전히 우선하지만 적힌 그대로 맞는 명령에만 적용되므로, 강제 푸시는 GitHub의 브랜치 보호로 막으세요."
        }
        "MCP servers" => "MCP 서버",
        "Open .mcp.json" => ".mcp.json 열기",
        "The servers in the project's .mcp.json. Claude may call the tools of those checked." => {
            "프로젝트의 .mcp.json에 있는 서버입니다. Claude는 체크한 서버의 도구만 부를 수 있습니다."
        }
        "Could not read .mcp.json: {}" => ".mcp.json을 읽지 못했습니다: {}",
        "It lists no servers." => "서버가 하나도 없습니다.",
        "Codex and Gemini do not read .mcp.json. An Organization agent playing a CCGS role uses only the tools its role file lists." => {
            "Codex와 Gemini는 .mcp.json을 읽지 않습니다. 조직 탭에서 CCGS 역할을 맡은 에이전트는 역할 파일에 적힌 도구만 씁니다."
        }
        "These choices stay on this computer, not in the project, so a copy of it starts with GitHub off." => {
            "이 선택은 프로젝트가 아니라 이 컴퓨터에 남습니다. 그래서 프로젝트를 복사하면 GitHub는 꺼진 채로 시작합니다."
        }
        "Save the scene first. The agent may change its file." => {
            "먼저 씬을 저장하세요. 에이전트가 씬 파일을 바꿀 수 있습니다."
        }
        // Organization
        "Low" => "낮음",
        "Medium" => "보통",
        "High" => "높음",
        "Max" => "최대",
        "Leadership" => "리더십",
        "Design" => "디자인",
        "Programming" => "프로그래밍",
        "Art & Audio" => "아트 & 오디오",
        "Quality" => "품질",
        "Operations" => "운영",
        "The links make a loop." => "링크가 순환합니다.",
        "Commands" => "명령",
        "Commands Claude may run without asking, in the chat and in Organization runs, one per line; * stands for any arguments. Agents need the ones that build and test the game, like cargo *, to finish their work. Claude asks in the chat before running others, and Always allow adds them here. A line like PowerShell(cargo *) is a rule for another of its tools. An agent playing a CCGS role runs them only if its role file lists Bash." => {
            "Claude가 묻지 않고 실행할 명령입니다. 채팅과 조직 실행 모두에 적용되고, 한 줄에 하나씩 적으며 *는 어떤 인자든 뜻합니다. 에이전트가 일을 끝내려면 cargo *처럼 게임을 빌드하고 테스트하는 명령이 필요합니다. 그 밖의 명령은 Claude가 실행하기 전에 채팅에서 묻고, 항상 허용하면 여기에 더해집니다. PowerShell(cargo *)처럼 적은 줄은 Claude의 다른 도구에 대한 규칙입니다. CCGS 역할을 맡은 에이전트는 역할 파일에 Bash가 있을 때만 실행합니다."
        }
        "The agent asks to use {}:" => "에이전트가 {} 사용을 허용해 달라고 합니다:",
        "Allow" => "허용",
        "Allowed in the chat; not in .mcp.json" => "채팅에서 허용함, .mcp.json에는 없음",
        "Always allow" => "항상 허용",
        "Deny" => "거부",
        "Saved in Settings > Agent Settings for this project." => {
            "이 프로젝트의 설정 > 에이전트 설정에 저장됩니다."
        }
        "Round {} of {}: {} agents go again." => "라운드 {}/{}: 에이전트 {}개가 다시 돕니다.",
        "Roles" => "역할",
        "Claude Code Game Studios' agents. Click one to add it." => {
            "Claude Code Game Studios의 에이전트입니다. 클릭하면 추가됩니다."
        }
        "+ Custom agent" => "+ 사용자 에이전트",
        "Group selection" => "선택 항목 묶기",
        "Puts a box around the selected agents." => "선택한 에이전트를 상자로 묶습니다.",
        "Center view" => "뷰 가운데로",
        "Drag from an agent's right pin to another agent to hand it the result. Right-click a pin to unlink it. Shift-click selects more. The mouse wheel zooms." => {
            "에이전트의 오른쪽 핀을 다른 에이전트로 드래그하면 결과를 넘깁니다. 핀을 오른쪽 클릭하면 연결을 끊습니다. Shift+클릭으로 여러 개를 선택합니다. 마우스 휠로 확대/축소합니다."
        }
        "Add agents from the roles on the left." => "왼쪽 역할 목록에서 에이전트를 추가하세요.",
        "Start with a studio" => "스튜디오로 시작",
        "Templates" => "템플릿",
        "Small studio" => "작은 스튜디오",
        "Direction, design, code and testing in a line." => {
            "디렉션, 기획, 코드, 테스트를 한 줄로 잇습니다."
        }
        "Every CCGS agent: directors hand the task to their department leads, leads to their specialists." => {
            "CCGS의 모든 에이전트: 디렉터가 부서 리드에게, 리드가 스페셜리스트에게 작업을 넘깁니다."
        }
        "Adds a ready-made team beside the agents already here." => {
            "지금 있는 에이전트 옆에 미리 짠 팀을 더합니다."
        }
        "Remove group" => "그룹 제거",
        "Takes the results of the agents linked into it. Right-click to unlink." => {
            "연결된 에이전트들의 결과를 받습니다. 오른쪽 클릭하면 연결을 끊습니다."
        }
        "Drag to an agent to hand it this one's result. Right-click to unlink." => {
            "에이전트로 드래그하면 이 에이전트의 결과를 넘깁니다. 오른쪽 클릭하면 연결을 끊습니다."
        }
        "That link would make a loop." => "그 링크는 순환을 만듭니다.",
        "Delete agent" => "에이전트 삭제",
        "Effort" => "추론 수준",
        "waiting" => "대기 중",
        "working" => "작업 중",
        "done" => "완료",
        "failed" => "실패",
        "Role" => "역할",
        "What this agent does in the team." => "이 에이전트가 팀에서 하는 일.",
        "Gemini's CLI has no effort setting; it is left out." => {
            "Gemini CLI에는 추론 수준 설정이 없어 생략됩니다."
        }
        "Last result" => "마지막 결과",
        "Nothing yet." => "아직 없습니다.",
        "Group" => "그룹",
        "Color" => "색상",
        "Keeps the agents inside." => "안의 에이전트는 남겨 둡니다.",
        "The organization has no agents yet. Add them in the Organization tab." => {
            "조직에 아직 에이전트가 없습니다. 조직 탭에서 추가하세요."
        }
        "Each agent answers in the chat, in link order. Set them up in the Organization tab." => {
            "에이전트마다 링크 순서대로 채팅에 답합니다. 에이전트는 조직 탭에서 구성하세요."
        }
        "There is no organization named \"{}\"." => "\"{}\" 조직이 없습니다.",
        "A name cannot be empty or hold \\ / : * ? \" < > |" => {
            "이름은 비울 수 없고 \\ / : * ? \" < > | 를 쓸 수 없습니다."
        }
        "Another organization has that name." => "같은 이름의 조직이 이미 있습니다.",
        "+ New organization" => "+ 새 조직",
        "Delete organization" => "조직 삭제",
        "Name" => "이름",
        "Thoughts" => "생각",
        "Continue" => "이어서",
        "Nothing to continue." => "이어서 할 작업이 없습니다.",
        "Runs the agents of {} that are not done yet, handing on what the others answered." => {
            "{}에서 아직 끝나지 않은 에이전트를 실행하고, 끝난 에이전트의 답을 넘깁니다."
        }
        // Project hub
        "Start creating" => "만들기 시작",
        "Create a new Besfa project or open one you already have." => {
            "새 Besfa 프로젝트를 만들거나 기존 프로젝트를 여세요."
        }
        "Create project" => "프로젝트 만들기",
        "Start a new binary Rust game project." => "새 바이너리 Rust 게임 프로젝트를 시작합니다.",
        "New project" => "새 프로젝트",
        "Open project" => "프로젝트 열기",
        "Open an existing Besfa project folder." => "기존 Besfa 프로젝트 폴더를 엽니다.",
        "Open folder" => "폴더 열기",
        "Recent projects" => "최근 프로젝트",
        "No recent projects" => "최근 프로젝트 없음",
        "Projects you open will appear here." => "연 프로젝트가 여기에 표시됩니다.",
        "Delete project" => "프로젝트 삭제",
        "Show in Explorer" => "탐색기에서 보기",
        "Open" => "열기",
        "Select location" => "위치 선택",
        "'{}' has no Cargo.toml." => "'{}'에 Cargo.toml이 없습니다.",
        "'{}' no longer exists." => "'{}'이(가) 더 이상 없습니다.",
        "'{}' is no longer a Cargo project and was removed." => {
            "'{}'은(는) 더 이상 Cargo 프로젝트가 아니어서 목록에서 뺐습니다."
        }
        "Could not delete '{}'." => "'{}'을(를) 삭제하지 못했습니다.",
        "Delete '{}'?" => "'{}'을(를) 삭제할까요?",
        "'{}' will be moved to the Recycle Bin." => "'{}'이(가) 휴지통으로 이동합니다.",
        "Enter the folder path for the new Cargo project." => {
            "새 Cargo 프로젝트의 폴더 경로를 입력하세요."
        }
        "Project directory" => "프로젝트 디렉터리",
        "🗁 Browse" => "🗁 찾아보기",
        "Add Claude Code Game Studios" => "Claude Code Game Studios 추가",
        "Creating…" => "만드는 중…",
        "➕ Create" => "➕ 만들기",
        "Enter a directory for the new project." => "새 프로젝트의 디렉터리를 입력하세요.",
        "Preparing Bevy…" => "Bevy 준비 중…",
        "Bevy prebuild paused for the project" => "프로젝트를 위해 Bevy 프리빌드를 멈췄습니다",
        "Bevy is ready" => "Bevy 준비 완료",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_falls_back_to_english() {
        assert_eq!(korean("Save"), Some("저장"));
        assert_eq!(korean("my_game"), None);
        // `t` keeps the English while the editor shows English.
        assert_eq!(t("Save"), "Save");
    }

    #[test]
    fn settings_round_trip() {
        let file = std::env::temp_dir().join(format!("besfa_settings_{}.ron", std::process::id()));
        let settings = Settings {
            language: Language::Korean,
        };
        settings.write(&file).unwrap();
        assert_eq!(Settings::read(&file), settings);
        let _ = fs::remove_file(&file);
        assert_eq!(Settings::read(&file), Settings::default());
    }
}
