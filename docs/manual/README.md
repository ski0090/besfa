# Besfa 사용 설명서

Besfa는 [Bevy](https://bevy.org/) 게임 엔진으로 게임을 만들 때 쓰는 Windows용 에디터입니다. 에디터 안에서 내 게임을 실제로 실행해 두고, 그 화면을 보면서 물건을 놓고, 값을 고치고, 지형을 빚고, 씬을 저장합니다.

이 설명서는 에디터를 처음 쓰는 사람을 위해 주제별로 나눠 썼습니다. 처음이라면 아래 "처음 읽는 순서"를 따라가세요. 필요한 기능만 찾는다면 [주제별 목차](#주제별-목차)나 [화면에서 바로 찾기](#화면에서-바로-찾기)를 쓰세요.

![Besfa 에디터 화면](../images/editor-overview.png)

## 처음 읽는 순서

1. [설치와 시작](01-getting-started.md): 에디터를 설치하고 첫 프로젝트를 만듭니다.
2. [화면 둘러보기](02-screen-layout.md): 메뉴, 툴바, 패널이 각각 무엇인지 봅니다.
3. [엔티티와 Inspector](03-hierarchy-inspector.md): 물건을 만들고 값을 고칩니다.
4. [씬 뷰 조작](04-scene-view.md): 카메라를 움직이고, 물건을 골라 옮깁니다.
5. [실행과 저장](05-play-and-save.md): 게임을 실행해 보고, 고친 씬을 저장합니다.

여기까지 읽으면 에디터의 기본은 모두 쓸 수 있습니다.

## 주제별 목차

| 주제 | 이런 걸 알고 싶을 때 |
| --- | --- |
| [설치와 시작](01-getting-started.md) | 설치, 에디터 실행, 프로젝트 만들기·열기·지우기 |
| [화면 둘러보기](02-screen-layout.md) | 화면의 각 부분 이름과 역할, 툴바 버튼 |
| [엔티티와 Inspector](03-hierarchy-inspector.md) | 엔티티 만들기·복제·삭제, 컴포넌트 값 고치기, 컴포넌트 추가·제거 |
| [씬 뷰 조작](04-scene-view.md) | 카메라 이동, 클릭으로 고르기, 이동·회전·크기 핸들, 맞춤(Snap), 땅에 내려놓기 |
| [실행과 저장](05-play-and-save.md) | Play와 Stop, 씬 저장, 코드 고치면 다시 빌드, 서버와 클라이언트로 실행 |
| [되돌리기](06-undo.md) | Undo와 Redo로 되돌릴 수 있는 것과 없는 것 |
| [에셋과 프리팹](07-assets-prefabs.md) | 모델 놓기, 썸네일, 다시 가져오기, 프리팹 만들기·고치기 |
| [모델 미리보기](08-model-preview.md) | 모델 하나만 따로 보기, 애니메이션 재생, 자세 잡기, 스크린샷 |
| [지형](09-terrain.md) | 지형 만들기, 높이 칠하기, 겉면 색 칠하기, 풀 기르기 |
| [흩뿌리기](10-scatter.md) | 나무·바위 같은 물건을 붓으로 흩어 놓기 |
| [애니메이션 상태 기계](11-animation.md) | 모델의 클립을 상태와 전이로 묶어 틀기 |
| [하늘](12-sky.md) | 씬의 하늘색, 해와 달, 큐브맵 |
| [업데이트](13-updating.md) | 에디터·CLI 업데이트, 게임의 플러그인 업데이트 |
| [Claude Code로 다루기](14-claude-code.md) | 에디터 없이 Claude Code가 씬을 고치게 하기 |
| [단축키](15-shortcuts.md) | 모든 단축키 한눈에 보기 |
| [문제 해결](16-troubleshooting.md) | Play가 눌리지 않을 때, 경고 아이콘, 자주 보는 메시지 |

## 화면에서 바로 찾기

화면에서 본 것이 무엇인지 궁금하면 여기서 찾아 들어가세요.

| 화면에서 본 것 | 설명 |
| --- | --- |
| 허브의 **New project**, **Open folder**, **Recent projects** | [설치와 시작](01-getting-started.md#프로젝트-허브) |
| 메뉴 **File**, **Edit**, **Window** | [화면 둘러보기 - 메뉴](02-screen-layout.md#1-메뉴) |
| 툴바 **▶ Play**, **⏹ Stop** | [실행과 저장](05-play-and-save.md#play와-stop) |
| 툴바 **Standalone** 목록 | [서버와 클라이언트로 실행](05-play-and-save.md#서버와-클라이언트로-실행) |
| 툴바 이동·회전·크기 버튼, **#**, **Grid** | [씬 뷰 조작 - 핸들](04-scene-view.md#이동회전크기-핸들) |
| 툴바 🌲 버튼 | [흩뿌리기](10-scatter.md) |
| 툴바 오른쪽 끝 ✔/⚠ 아이콘 | [업데이트 - 게임의 플러그인](13-updating.md#게임의-플러그인-업데이트) |
| 툴바 오른쪽 끝 **Bevy is ready** | [설치와 시작 - Bevy 미리 빌드](01-getting-started.md#bevy-미리-빌드) |
| 왼쪽 **Hierarchy** 패널 | [엔티티와 Inspector](03-hierarchy-inspector.md#hierarchy-패널) |
| 오른쪽 **Inspector** 탭 | [엔티티와 Inspector](03-hierarchy-inspector.md#inspector-패널) |
| 오른쪽 **Scatter** 탭 | [흩뿌리기](10-scatter.md) |
| 오른쪽 **Animation** 탭 | [애니메이션 상태 기계](11-animation.md) |
| 아래 **Log** 탭 | [화면 둘러보기 - 아래 패널](02-screen-layout.md#6-아래-패널-log와-assets) |
| 아래 **Assets** 탭 | [에셋과 프리팹](07-assets-prefabs.md) |
| 뷰포트 위 **Scene** \| **모델 이름** 탭 | [모델 미리보기](08-model-preview.md) |
| 창 제목의 `*` | [실행과 저장 - 씬 저장](05-play-and-save.md#씬-저장) |

## 용어

설명서에서 자주 쓰는 말입니다. Bevy를 처음 쓴다면 한 번 읽어 두세요.

| 용어 | 뜻 |
| --- | --- |
| 프로젝트 | 내가 만드는 게임 하나. Rust(Cargo) 프로젝트 폴더 하나입니다. |
| 씬 | 게임이 시작할 때 만드는 물건들의 목록. `assets/scenes/main.scn.ron` 파일 하나에 저장됩니다. |
| 엔티티 | 씬에 있는 물건 하나(큐브, 조명, 카메라, 캐릭터 등). |
| 컴포넌트 | 엔티티에 붙은 데이터 한 덩어리. 위치는 `Transform`, 보임 여부는 `Visibility`처럼 컴포넌트마다 하는 일이 있습니다. |
| 시스템 | 게임 코드의 함수. 매 프레임 컴포넌트를 읽고 바꿉니다(예: 큐브를 돌리는 `spin`). |
| 에셋 | 프로젝트의 `assets` 폴더에 있는 파일. 3D 모델(`.glb`, `.gltf`), 씬과 프리팹(`.scn.ron`) 등. |
| 프리팹 | 여러 엔티티를 묶어 파일로 저장한 것. 씬에 여러 번 놓고, 고치면 놓은 곳 모두에 반영됩니다. |
| 편집 상태 | 게임이 멈춘 채로 떠 있는 상태. 씬을 고치는 동안은 이 상태입니다. |
| 실행 상태 | Play를 눌러 게임 시간이 흐르는 상태. 게임을 직접 해 볼 수 있습니다. |
| 플러그인 | 게임에 들어가 에디터와 대화하는 Rust 크레이트 `besfa_editor_plugin`. 프로젝트마다 버전이 고정됩니다. |
| Bevy 미리 빌드 | 에디터가 시작할 때 Bevy를 미리 컴파일해 두는 작업. 덕분에 게임은 내 코드만 컴파일합니다. |

## 개발자 문서

설명서는 쓰는 법만 다룹니다. 에디터가 안에서 어떻게 동작하는지, 파일 형식과 통신 규약은 개발자 문서를 보세요.

- [프로젝트 개요](../PROJECT_OVERVIEW.md), [아키텍처](../ARCHITECTURE.md), [Editor UI 명세](../EDITOR_UI.md)
- [에디터 데이터 포맷](../DATA_FORMAT.md), [CLI 계약](../CLI_CONTRACT.md)

## 설명서 스크린샷 다시 찍기

설명서의 에디터 스크린샷은 테스트가 그립니다. UI를 바꾼 뒤에는 저장소 폴더에서 아래 명령으로 다시 찍으세요(`docs/images`에 씁니다).

```bash
cargo test -p besfa_editor manual_screenshots -- --ignored
```

테스트는 실제 게임이 보낸 보고(`crates/besfa_editor/screenshot_data`)를 화면에 넣어 그립니다. 뷰포트에는 게임 화면이 그려지지 않으므로 뷰포트를 보여 주는 그림은 없습니다.
