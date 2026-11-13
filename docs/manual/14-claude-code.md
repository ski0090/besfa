[← 사용 설명서 목차](README.md)

# Claude Code로 다루기

[Claude Code](https://claude.com/claude-code)에게 말로 씬 작업을 맡길 수 있습니다. Claude Code가 에디터 없이 게임을 띄워, 씬에 물건을 놓고, 땅을 빚고, 흩뿌리고, 애니메이션 상태 기계를 짜고, 씬 사진을 찍습니다.

## 준비

`besfa new`(또는 허브의 **New project**)로 만든 프로젝트에는 이미 `.mcp.json` 파일이 있습니다. 그 프로젝트 폴더에서 Claude Code를 열면 바로 씁니다.

예전에 만든 프로젝트라면, 프로젝트 폴더에 `.mcp.json` 파일을 만들고 아래 내용을 넣으세요.

```json
{ "mcpServers": { "besfa": { "command": "besfa", "args": ["mcp"] } } }
```

`besfa` 명령이 설치되어 있어야 합니다([설치와 시작](01-getting-started.md#설치)).

## Claude Code Game Studios 더하기

[Claude Code Game Studios](https://github.com/ski0090/Claude-Code-Game-Studios)(CCGS)는 Claude Code로 게임을 만들 때 쓰는 에이전트, 스킬, 훅 모음입니다. 기획 문서, 스프린트, 리뷰, 테스트 같은 개발 과정을 슬래시 명령으로 이끌고, Bevy 0.20 레퍼런스와 `bevy-specialist` 에이전트가 들어 있습니다.

- 허브에서 새 프로젝트를 만들 때 **Add Claude Code Game Studios**를 켜거나, 터미널에서 `besfa new <폴더> --ccgs`로 만듭니다.
- **Git**이 있어야 하고, 만들 때 인터넷으로 CCGS를 받습니다. 받지 못하면 프로젝트를 만들지 않고 이유를 보여 줍니다.
- CCGS는 프로젝트의 `.ccg` 폴더에 git 서브모듈로 들어가고, `.claude/` 등에는 그리로 가는 링크가 생깁니다. Windows에서 링크를 만들려면 **개발자 모드**가 켜져 있어야 합니다(설정 > 시스템 > 개발자용).
- 이 프로젝트를 다른 PC에서 받을 때는 `git clone --recurse-submodules`로 받습니다. CCGS를 새 버전으로 올리려면 `git submodule update --remote .ccg`를 실행하고 커밋합니다.
- 만든 뒤 프로젝트 폴더에서 Claude Code를 열고 `/start`, 이어서 `/setup-engine bevy 0.20`을 실행하면 설정이 끝납니다.

## 쓰는 법

프로젝트 폴더에서 Claude Code를 열고 하고 싶은 일을 말하면 됩니다. 예:

- "씬에 있는 엔티티를 보여 줘"
- "`assets/models/oak-tree.glb`를 원점 근처에 다섯 그루 놓아 줘"
- "지형을 만들고 가운데를 언덕으로 올려 줘"
- "zombie 모델에 idle·walk 상태 기계를 붙여 줘"
- "씬 사진을 찍어 줘"
- "Play하고 플레이어가 보는 화면을 UI까지 찍어 줘"

Claude Code는 `besfa mcp`로 게임을 **창 없이** 편집 상태로 띄우고, 에디터가 쓰는 것과 같은 명령을 게임에 보냅니다. 화면을 띄우지 않으므로 작업 중인 다른 창을 가리지 않습니다. 창이 없어도 사진은 찍힙니다. 씬을 원하는 곳에서 본 사진과, 게임 카메라가 보는 그대로(게임의 UI 포함) 찍은 사진 둘 다 됩니다. 게임에 실행 인자가 있으면(예: 특정 레벨로 시작) 그 인자로 띄울 수도 있습니다. 터미널에서는 `besfa screenshot shots/hud.png`가 게임을 창 없이 띄워 Play하고 플레이어가 보는 화면을 그 파일에 남깁니다. 파일은 프로젝트 안이어야 하고, `--wait 3`처럼 찍기 전에 몇 초 놀게 할 수 있으며, `--` 뒤의 인자는 게임이 받습니다. 사진을 남기지 못하면 종료 코드 30으로 끝납니다.

`besfa mcp`는 한 번에 게임 하나만 띄웁니다. Claude Code를 닫으면 띄운 게임도 함께 꺼집니다.

## 이미지 만들기

Claude는 그림을 그리지 못하므로, `besfa mcp`가 OpenAI의 이미지 API로 그림을 그려 프로젝트에 PNG로 저장합니다. UI 아이콘, 버튼, 패널 조각에 씁니다. 예: "`assets/ui/coin.png`에 금화 아이콘을 투명 배경으로 그려 줘".

- **준비**: OpenAI API 키를 사용자나 시스템 환경 변수 `OPENAI_API_KEY`로 설정하고, Claude Code나 에디터를 다시 시작합니다. ChatGPT 구독은 필요 없고, API는 [OpenAI 플랫폼](https://platform.openai.com)에서 쓴 만큼 따로 결제합니다. 키는 `curl`(Windows에 들어 있음)에 표준 입력으로만 넘어가고 파일이나 명령줄에 남지 않습니다.
- **비용**: 그릴 때마다 OpenAI 계정에 요금이 붙습니다. 요금은 쓴 토큰으로 매겨지고, 결과에 그 토큰 수가 나옵니다. 품질과 크기를 올리면 토큰이 늘어납니다. 시험해 본 낮은 품질의 1024x1024 투명 아이콘 한 장은 출력 토큰 824개였습니다. 정확한 단가는 OpenAI의 가격 페이지를 보세요. OpenAI 플랫폼의 **Usage limits**에서 월 한도를 걸어 두세요.
- **고를 수 있는 것**: 크기(`1024x1024`, `1536x1024`, `1024x1536`), 품질(`low`부터 `high`), 배경(`transparent`, `opaque`). 기본 모델은 투명 배경을 그리는 `gpt-image-2.6-flare`입니다. `gpt-image-2`는 투명 배경을 거절합니다.
- **모델 업데이트**: OpenAI가 새 이미지 모델을 내면 Claude에게 "쓸 수 있는 이미지 모델을 보여 줘"라고 하세요(`image_models` 도구, 무료). 고른 모델 이름을 말하면 그 모델로 그립니다. Besfa를 업데이트하지 않아도 됩니다. 기본 모델 자체는 Besfa를 [업데이트](13-updating.md)할 때 바뀝니다.
- 파일은 프로젝트 안의 `.png`여야 하고, 폴더가 없으면 만들며, 같은 이름의 파일은 덮어씁니다.
- 에디터의 Agent 창에서는 **Settings** 탭에서 `besfa` 서버가 체크되어 있으면(처음부터 체크됨) 채팅과 조직의 모든 Claude 에이전트가 **묻지 않고** 그립니다. 그래서 위의 월 한도가 비용을 막는 유일한 장치입니다.
- [조직](15-agent-window.md#organization-탭)에서 쓸 때는 Claude 에이전트에게 맡깁니다. Codex와 Gemini는 `besfa` 서버를 읽지 않습니다.

## 빌드와 테스트

Claude Code(또는 터미널)가 게임을 빌드하거나 테스트할 때는 `cargo` 대신 아래 명령을 쓰게 하세요. 에디터의 Run과 같은 설정으로 빌드하므로, 이미 받아 둔 Bevy 빌드를 다시 쓰고 몇 분씩 다시 빌드하지 않습니다.

| 명령 | 하는 일 |
| --- | --- |
| `besfa build` | 게임을 빌드합니다. |
| `besfa test` | 테스트를 빌드하고 실행합니다. |
| `besfa run` | 게임을 창으로, 플레이어처럼(편집 상태가 아니라) 띄웁니다. 게임이 끝나면 게임의 종료 코드로 끝납니다. |

`--` 앞의 인자는 cargo가 받고(`besfa build --release`, `besfa test my_test`), `--` 뒤의 인자는 테스트나 게임이 받습니다.

빌드가 실패하면 종료 코드 30, 테스트가 실패하면 101로 끝나므로, 둘을 코드로 구분할 수 있습니다.

## 주의

- **에디터와 Claude Code가 같은 프로젝트를 동시에 다루지 마세요.** 둘 다 씬 파일을 쓰므로 한쪽의 변경이 다른 쪽에 덮어쓰입니다. 하나를 닫고 다른 쪽을 쓰세요.
- 에디터의 **Agent 창**에서 Claude에게 맡기면, 그 Claude도 프로젝트의 `.mcp.json`으로 `besfa mcp`를 씁니다. 이때는 에디터를 닫지 않아도 됩니다. 쓰는 법과 주의는 [Agent 창](15-agent-window.md)을 보세요.
- Claude Code가 쓰는 `besfa mcp`는 CLI를 설치할 때 알던 명령만 받고, 모르는 명령은 거절합니다. 게임의 플러그인을 업데이트해 새 명령이 생겼다면 CLI도 다시 설치하세요([업데이트](13-updating.md#에디터와-cli-업데이트)).

---

다음: [Agent 창](15-agent-window.md)
