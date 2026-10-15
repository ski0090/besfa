# Besfa 프로젝트 개요

## 목적

Besfa는 Bevy 게임 엔진을 위한 데스크톱 에디터 프로젝트다.

에디터 UI는 Rust와 egui로 만들고, 게임 프로젝트의 Bevy 런타임과는 별도 프로세스로 연결한다. 에디터는 패널 UI, 프로젝트 관리, 씬 편집 도구를 담당한다. Bevy는 게임 실행, ECS 상태, 씬 렌더링을 담당한다.

Windows에서는 Bevy가 에디터의 D3D12 공유 텍스처에 뷰포트를 렌더링한다. 연결 경로와 프로세스 수명은 [ARCHITECTURE.md](./ARCHITECTURE.md)를 참고한다.

## 프로젝트 경계

Besfa 에디터와 Besfa로 생성한 게임은 서로 독립된 프로젝트다.

- 이 저장소는 에디터, CLI, 공통 SDK를 관리한다.
- 사용자가 만드는 게임은 별도 폴더와 별도 Git 저장소를 가진다.
- 게임 프로젝트는 필요한 Besfa Rust crate만 의존성으로 사용한다.

```text
besfa/                         # 이 저장소
└─ templates/                  # 향후 게임 생성용 템플릿

my-game/                       # 에디터가 생성하는 별도 사용자 프로젝트
├─ Cargo.toml
├─ src/
├─ assets/
└─ besfa/                      # .besfa 프로젝트 데이터
```

## 현재 구성

### `crates/besfa_editor`

egui(eframe, wgpu DX12) 기반 Windows 데스크톱 에디터다. 프로젝트 허브와, 게임을 뷰포트에서 실행하며 씬을 고치는 편집 화면이 있다. 역할과 동작은 [EDITOR_UI.md](./EDITOR_UI.md)를 참고한다.

### `crates/besfa_protocol`

에디터와 게임의 `besfa_editor_plugin`이 함께 쓰는 메시지 타입이다. 에디터가 게임의 stdin에 쓰는 `Request`(`Command`와 답을 받을 `seq`)와 게임이 stdout에 쓰는 `Report`, 그리고 프로토콜 버전이 있다. 자세한 규칙은 [ARCHITECTURE.md](./ARCHITECTURE.md)를 참고한다.

### `crates/besfa_editor_plugin`

게임 프로젝트가 의존하는 Bevy 플러그인이다. 씬 파일을 읽고, 에디터가 띄우면 편집 상태로 시작해 에디터의 명령을 받는다. 게임 코드가 쓰는 것으로 지형이 있다: 지형은 `Terrain`(크기와 겉면 색)과 `TerrainFile`(높이 파일 경로)로 씬에 저장되고, 게임 코드는 시스템 인자 `TerrainHeight`의 `at(x, z)`로 그 자리의 땅 높이를 묻는다(캐릭터와 좀비를 땅 위에 세울 때).

### `crates/besfa_mcp`

에이전트(Claude Code 등)가 에디터 UI 없이 게임의 씬을 읽고 고치게 하는 stdio MCP 서버다. 에디터의 Run처럼 게임을 edit mode로 띄우고(뷰포트도 창도 없이, `BESFA_HEADLESS`로 사용자 포커스를 빼앗지 않는다. 모델 썸네일은 오프스크린으로 그대로 그려진다) `besfa_protocol` 명령을 그대로 전달한다. 도구는 `start`, `send`(명령 하나나 목록을 차례로 보내고, `screenshot`·`thumbnail`·`reimport`는 답이 올 때까지 기다린다. 게임의 프로토콜이 모르는 명령은 보내기 전에 거절한다), `entities`(이름·에셋·부모로 거르고, 놓인 모델이 펼친 부품은 빼거나 함께 본다, `id`면 그 엔티티의 컴포넌트), `stop`이고, 저장소 루트의 `.mcp.json`이 `cargo run -q -p besfa_mcp`로 등록한다. 에디터가 열어 둔 프로젝트에 같이 쓰면 두 쪽이 씬 파일을 덮어쓴다.

### `crates/besfa_cli`

Besfa 프로젝트 자동화를 위한 Rust 바이너리 crate다.

현재 정의된 명령은 다음과 같다.

```text
besfa new <DIRECTORY> [--template <TEMPLATE>]
besfa validate [DIRECTORY]
besfa --help
besfa --version
```

`new`와 `validate`는 인자 파싱과 도움말만 구현되어 있으며, 실제 파일 생성과 검증은 아직 구현하지 않았다.

## 예정 폴더 구조

```text
besfa/
├─ crates/
│  ├─ besfa_editor/                 # egui 데스크톱 에디터
│  ├─ besfa_cli/                    # `besfa` 명령행 도구
│  ├─ besfa_project/                # 프로젝트 생성, 검증, 템플릿 처리
│  ├─ besfa_data/                   # .besfa 파일 포맷, 파싱, 검증, 마이그레이션
│  ├─ besfa_protocol/               # 에디터와 게임 런타임 간 통신 규약
│  ├─ besfa_editor_plugin/          # 게임 프로젝트에 추가하는 Bevy Plugin
│  └─ besfa_viewport_windows/       # D3D 공유 텍스처 기반 Windows 뷰포트
│
├─ templates/
│  └─ basic_3d/                     # 첫 게임 프로젝트 템플릿
│
├─ docs/                            # 설계와 개발 문서
├─ Cargo.toml                       # Rust workspace
└─ Cargo.lock
```

빈 디렉터리는 미리 만들지 않는다. 각 기능을 실제로 시작할 때 생성한다.

## 책임 분리

| 구성 요소 | 책임 |
| --- | --- |
| `besfa_editor` | UI 표시, 사용자 입력, 프로젝트 생성 요청, 게임 프로세스 관리 |
| `besfa_cli` | 터미널과 CI에서 실행하는 명령 인터페이스 |
| `besfa_project` | 프로젝트 생성과 파일 시스템 작업의 실제 규칙 |
| `besfa_data` | 에디터 데이터 파일의 읽기, 쓰기, 검증 |
| `besfa_editor_plugin` | `.besfa` 데이터와 Bevy ECS/Asset의 연결 |
| `besfa_protocol` | 에디터와 실행 중 게임 사이의 메시지 타입 |
| `besfa_viewport_windows` | Bevy 렌더 타깃과 에디터 뷰포트의 GPU 연결 |

## 설계 문서

- [아키텍처](./ARCHITECTURE.md)
- [Editor UI](./EDITOR_UI.md)
- [CLI 계약](./CLI_CONTRACT.md)
- [에디터 데이터 포맷](./DATA_FORMAT.md)
- [TODO](./TODO.md)
