[← 사용 설명서 목차](README.md)

# 문제 해결

자주 겪는 상황과 Log에 나오는 메시지의 뜻을 모았습니다. 문제가 생기면 먼저 아래 **Log** 탭을 보세요. 대부분의 이유가 거기 나옵니다.

- [Play 단추가 눌리지 않아요](#play-단추가-눌리지-않아요)
- [Bevy 미리 빌드가 실패해요](#bevy-미리-빌드가-실패해요)
- [프로젝트가 만들어지지 않아요](#프로젝트가-만들어지지-않아요)
- [게임이 빌드되지 않아요](#게임이-빌드되지-않아요)
- [툴바에 노란 ⚠가 보여요](#툴바에-노란-가-보여요)
- [메뉴나 기능이 설명서와 달라요](#메뉴나-기능이-설명서와-달라요)
- [게임이 따로 창으로 떠요](#게임이-따로-창으로-떠요)
- [게임 화면이 느려요](#게임-화면이-느려요)
- [고친 값이 사라졌어요](#고친-값이-사라졌어요)

## Play 단추가 눌리지 않아요

- 툴바 오른쪽 끝을 보세요. [Bevy 미리 빌드](01-getting-started.md#bevy-미리-빌드)가 진행 중이면 끝날 때까지 Play가 꺼져 있습니다. **Bevy is ready**가 뜨면 눌립니다.
- 게임을 끄고 다시 띄우는 중이거나 플러그인을 업데이트하는 중에도 잠시 꺼집니다.

## Bevy 미리 빌드가 실패해요

툴바(또는 허브 아래)에 실패한 종료 코드가 보이면, 터미널에서 직접 실행해 전체 에러를 확인하세요.

```bash
besfa prebuild
```

`besfa`가 오래된 것이 원인일 수 있습니다. [CLI를 다시 설치](13-updating.md#에디터와-cli-업데이트)하고 에디터를 다시 시작해 보세요.

## 프로젝트가 만들어지지 않아요

**Create project** 창에 나온 빨간 글씨를 보세요.

- `Could not start the Besfa CLI.`: `besfa` 명령을 찾지 못했습니다. [CLI를 설치](01-getting-started.md#설치)하고, 에디터를 다시 시작하세요.
- `The destination directory already exists.`: 새 프로젝트 폴더는 아직 없는 경로여야 합니다. 다른 이름을 쓰세요.

## 게임이 빌드되지 않아요

- **Log**에 cargo의 컴파일 에러가 나옵니다. 내 게임 코드(`src`)의 에러라면 고쳐서 저장하세요. 에디터가 [저절로 다시 빌드](05-play-and-save.md#코드를-고치면-다시-빌드)합니다.
- 플러그인 쪽 에러라면 [플러그인을 업데이트](13-updating.md#게임의-플러그인-업데이트)해 보세요.

## 툴바에 노란 ⚠가 보여요

게임의 플러그인이 Bevy 미리 빌드와 다르거나, 에디터와 대화하는 방식(프로토콜 버전)이 다르다는 뜻입니다. 아이콘에 마우스를 올려 이유를 보고, 누르면 업데이트합니다. [업데이트](13-updating.md#게임의-플러그인-업데이트)

Log에 이런 메시지가 나올 수 있습니다.

| 메시지 | 할 일 |
| --- | --- |
| `... speaks protocol N, older than this editor's M. Update the plugin.` | 툴바의 ⚠를 눌러 플러그인을 업데이트하세요. |
| `... speaks protocol N, newer than this editor's M. Update the editor.` | 게임의 플러그인이 에디터보다 새것입니다. [에디터를 업데이트](13-updating.md#에디터와-cli-업데이트)하세요. |
| `... predates protocol versions ... Update the plugin.` | 아주 오래된 플러그인입니다. ⚠를 눌러 업데이트하세요. |

## 메뉴나 기능이 설명서와 달라요

Hierarchy의 **➕** 메뉴에 **Terrain**이나 **Sky**가 없거나, 🌲 흩뿌리기·땅에 내려놓기 같은 기능이 꺼져 있다면 대개 게임의 플러그인이 오래된 것입니다. [플러그인을 업데이트](13-updating.md#게임의-플러그인-업데이트)하세요.

모델 미리보기를 열 때 `The model preview needs the game running with besfa_editor_plugin speaking protocol 3 or later. Update the plugin.`이 나와도 같습니다. 게임이 아직 떠 있지 않을 때도 이 메시지가 나오니, 게임이 뜬 뒤에 다시 해 보세요.

## 게임이 따로 창으로 떠요

에디터가 뷰포트용 화면을 만들지 못하면 게임이 자기 창으로 뜹니다. Log에 `Viewport unavailable, the game opens its own window: <이유>`가 나옵니다. 그래픽 드라이버가 DirectX 12를 지원하는지 확인하세요.

## 게임 화면이 느려요

Log에 `No Windows SDK DXC found: the game compiles its shaders with FXC, which can draw much slower.`가 있다면, Windows SDK가 없어 셰이더를 느린 방식으로 컴파일하고 있다는 뜻입니다. Visual Studio 설치 프로그램 등으로 **Windows SDK**를 설치한 뒤 에디터를 다시 시작하세요. 일부 그래픽 카드(예: Intel 내장 그래픽)에서 차이가 큽니다.

## 고친 값이 사라졌어요

- **Stop**을 누르면 씬이 파일에 저장된 모습으로 돌아갑니다. 실행 중(Play)에 바꾼 것은 저장되지 않습니다. [실행과 저장](05-play-and-save.md#편집-상태와-실행-상태)
- 편집 상태에서 고친 것은 **Save scene**(Ctrl+S)으로 저장해야 다음에 열 때도 남습니다. 창 제목에 `*`가 있으면 아직 저장하지 않은 것입니다.
- 프리팹 안쪽을 고친 것은 Save scene이 아니라 [Apply to prefab](07-assets-prefabs.md#프리팹-고치기)으로 저장합니다.

---

[← 사용 설명서 목차](README.md)
